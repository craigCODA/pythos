//! Bounded presentation records for one verified graph-window worker.

use crate::{
    capability_abi::PackedCapability,
    pyth_runtime_abi::{GRAPH_EXIT_OK, GRAPH_RESULT_UNIT, GraphExitRecord},
    pyth_tig::{
        opcode::{Opcode, RESOURCE_OBJECT_WORKSPACE, RIGHTS_CREATE},
        verify::VerifiedGraph,
    },
    user_program_manifest::digest64,
};

pub const GRAPH_NAME: &[u8] = b"graph-window.tig";
pub const WORKER_NAME: &[u8] = b"graph-window.elf";
pub const PRINCIPAL: u64 = 0x5059_5749_4e44_0001;
pub const INVOCATION_LEN: usize = 4096;
pub const RESULT_LEN: usize = 1024;
pub const MAX_PACKAGE_LEN: usize = INVOCATION_LEN - INVOCATION_HEADER_LEN;
pub const INSTRUCTION_BUDGET: u64 = 128;
pub const MAX_NODES: usize = 12;
pub const MAX_WIDTH: u16 = 384;
pub const MAX_HEIGHT: u16 = 240;
pub const MAX_TEXT_BYTES: usize = 32;
pub const KEYFRAME_COUNT: usize = 4;

const INVOCATION_MAGIC: &[u8; 8] = b"PYGWIN01";
const RESULT_MAGIC: &[u8; 8] = b"PYGWFR01";
const INVOCATION_HEADER_LEN: usize = 32;
const RESULT_HEADER_LEN: usize = 80;
const NODE_LEN: usize = 48;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowError {
    Invocation,
    GraphPolicy,
    GraphExecution,
    Scene,
    Result,
}

pub struct Invocation<'a> {
    pub package: &'a [u8],
    pub workspace: PackedCapability,
}

impl<'a> Invocation<'a> {
    pub fn encode(
        package: &[u8],
        workspace: PackedCapability,
    ) -> Result<[u8; INVOCATION_LEN], WindowError> {
        if package.is_empty() || package.len() > MAX_PACKAGE_LEN || workspace.generation() == 0 {
            return Err(WindowError::Invocation);
        }
        let mut bytes = [0; INVOCATION_LEN];
        bytes[..8].copy_from_slice(INVOCATION_MAGIC);
        bytes[8..10].copy_from_slice(&(package.len() as u16).to_le_bytes());
        bytes[16..24].copy_from_slice(&workspace.raw().to_le_bytes());
        bytes[24..32].copy_from_slice(&digest64(package).to_le_bytes());
        bytes[INVOCATION_HEADER_LEN..INVOCATION_HEADER_LEN + package.len()]
            .copy_from_slice(package);
        Ok(bytes)
    }

    pub fn decode(bytes: &'a [u8; INVOCATION_LEN]) -> Result<Self, WindowError> {
        let len = usize::from(read_u16(bytes, 8));
        let workspace = PackedCapability::from_raw(read_u64(bytes, 16));
        if &bytes[..8] != INVOCATION_MAGIC
            || !zero(&bytes[10..16])
            || len == 0
            || len > MAX_PACKAGE_LEN
            || workspace.generation() == 0
        {
            return Err(WindowError::Invocation);
        }
        let end = INVOCATION_HEADER_LEN + len;
        let package = &bytes[INVOCATION_HEADER_LEN..end];
        if !zero(&bytes[end..]) || digest64(package) != read_u64(bytes, 24) {
            return Err(WindowError::Invocation);
        }
        Ok(Self { package, workspace })
    }
}

/// Check the projection's host-operation policy after shared PythTIG admission.
pub fn validate_graph(verified: &VerifiedGraph<'_>) -> Result<(), WindowError> {
    let package = verified.package();
    let imports = package.imports();
    let import = imports.get(0).ok_or(WindowError::GraphPolicy)?;
    if package.header().principal_id != PRINCIPAL
        || package.blocks().len() != 1
        || package.nodes().len() as u64 > INSTRUCTION_BUDGET
        || imports.len() != 1
        || import.import_slot != 0
        || import.resource_kind != RESOURCE_OBJECT_WORKSPACE
        || import.rights != RIGHTS_CREATE
        || !package.nodes().iter().all(|node| {
            matches!(
                Opcode::try_from(node.opcode),
                Ok(Opcode::BlockParam
                    | Opcode::EffectStart
                    | Opcode::ConstUtf8
                    | Opcode::ConstBytes
                    | Opcode::HostResult
                    | Opcode::ObjectCreate
                    | Opcode::ObjectRevise
                    | Opcode::Return)
            )
        })
    {
        return Err(WindowError::GraphPolicy);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    fn fits(self, width: u16, height: u16) -> bool {
        self.width != 0
            && self.height != 0
            && self
                .x
                .checked_add(self.width)
                .is_some_and(|edge| edge <= width)
            && self
                .y
                .checked_add(self.height)
                .is_some_and(|edge| edge <= height)
    }

    fn decode(bytes: &[u8]) -> Self {
        Self {
            x: read_u16(bytes, 0),
            y: read_u16(bytes, 2),
            width: read_u16(bytes, 4),
            height: read_u16(bytes, 6),
        }
    }

    fn encode(self, bytes: &mut [u8]) {
        for (field, value) in
            bytes
                .chunks_exact_mut(2)
                .zip([self.x, self.y, self.width, self.height])
        {
            field.copy_from_slice(&value.to_le_bytes());
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Node {
    pub rect: Rect,
    pub color: u32,
    text_len: u8,
    text: [u8; MAX_TEXT_BYTES],
}

impl Node {
    pub const EMPTY: Self = Self::rectangle(
        Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        },
        0,
    );

    pub const fn rectangle(rect: Rect, color: u32) -> Self {
        Self {
            rect,
            color,
            text_len: 0,
            text: [0; MAX_TEXT_BYTES],
        }
    }

    pub fn label(rect: Rect, color: u32, text: &[u8]) -> Result<Self, WindowError> {
        if text.is_empty() || text.len() > MAX_TEXT_BYTES {
            return Err(WindowError::Scene);
        }
        let mut node = Self::rectangle(rect, color);
        node.text_len = text.len() as u8;
        node.text[..text.len()].copy_from_slice(text);
        if !node.valid_style() {
            return Err(WindowError::Scene);
        }
        Ok(node)
    }

    pub fn text(&self) -> Option<&str> {
        (self.text_len != 0)
            .then(|| core::str::from_utf8(&self.text[..usize::from(self.text_len)]).ok())
            .flatten()
    }

    fn valid_style(self) -> bool {
        let len = usize::from(self.text_len);
        len <= MAX_TEXT_BYTES
            && self.color <= 0xFF_FF_FF
            && zero(&self.text[len..])
            && (len == 0 || (len * 8 <= usize::from(self.rect.width) && self.rect.height >= 8))
            && self.text[..len]
                .iter()
                .all(|byte| matches!(byte, b' '..=b'~'))
    }

    fn decode(bytes: &[u8]) -> Self {
        Self {
            rect: Rect::decode(&bytes[..8]),
            color: read_u32(bytes, 8),
            text_len: bytes[12],
            text: bytes[16..48].try_into().unwrap(),
        }
    }

    fn encode(self, bytes: &mut [u8]) {
        self.rect.encode(&mut bytes[..8]);
        bytes[8..12].copy_from_slice(&self.color.to_le_bytes());
        bytes[12] = self.text_len;
        bytes[16..48].copy_from_slice(&self.text);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Keyframe {
    pub frame: u16,
    pub x: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Animation {
    pub node_index: u8,
    pub period_frames: u16,
    pub keyframes: [Keyframe; KEYFRAME_COUNT],
}

impl Animation {
    fn fits(self, node: Node, width: u16) -> bool {
        self.keyframes[0]
            == (Keyframe {
                frame: 0,
                x: node.rect.x,
            })
            && self.keyframes[KEYFRAME_COUNT - 1].frame < self.period_frames
            && self
                .keyframes
                .windows(2)
                .all(|pair| pair[0].frame < pair[1].frame)
            && self.keyframes.iter().any(|key| key.x != node.rect.x)
            && self.keyframes.iter().all(|key| {
                key.x
                    .checked_add(node.rect.width)
                    .is_some_and(|edge| edge <= width)
            })
    }

    fn sample(self, frame: u64) -> Option<u16> {
        let phase = frame.checked_rem(u64::from(self.period_frames))? as u16;
        let first = self.keyframes[0];
        let mut start = first;
        let repeat = Keyframe {
            frame: self.period_frames,
            x: first.x,
        };
        for end in self.keyframes.into_iter().skip(1).chain([repeat]) {
            if phase < end.frame {
                let elapsed = i64::from(phase.checked_sub(start.frame)?);
                let duration = i64::from(end.frame - start.frame);
                let start_x = i64::from(start.x);
                let travel = (i64::from(end.x) - start_x) * elapsed / duration;
                return u16::try_from(start_x + travel).ok();
            }
            start = end;
        }
        None
    }

    fn decode(bytes: &[u8]) -> Self {
        Self {
            node_index: bytes[0],
            period_frames: read_u16(bytes, 2),
            keyframes: core::array::from_fn(|index| Keyframe {
                frame: read_u16(bytes, 4 + index * 4),
                x: read_u16(bytes, 6 + index * 4),
            }),
        }
    }

    fn encode(self, bytes: &mut [u8]) {
        bytes[0] = self.node_index;
        bytes[2..4].copy_from_slice(&self.period_frames.to_le_bytes());
        for (pair, key) in bytes[4..].chunks_exact_mut(4).zip(self.keyframes) {
            pair[..2].copy_from_slice(&key.frame.to_le_bytes());
            pair[2..].copy_from_slice(&key.x.to_le_bytes());
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Scene {
    pub width: u16,
    pub height: u16,
    pub title_bar: Rect,
    pub animation: Animation,
    node_count: u8,
    nodes: [Node; MAX_NODES],
}

impl Scene {
    pub fn new(
        width: u16,
        height: u16,
        title_bar: Rect,
        nodes: &[Node],
        animation: Animation,
    ) -> Result<Self, WindowError> {
        let animated = nodes
            .get(usize::from(animation.node_index))
            .ok_or(WindowError::Scene)?;
        if width > MAX_WIDTH
            || height > MAX_HEIGHT
            || !title_bar.fits(width, height)
            || nodes.len() > MAX_NODES
            || !nodes
                .iter()
                .all(|node| node.valid_style() && node.rect.fits(width, height))
            || !animation.fits(*animated, width)
        {
            return Err(WindowError::Scene);
        }
        let mut owned_nodes = [Node::EMPTY; MAX_NODES];
        owned_nodes[..nodes.len()].copy_from_slice(nodes);
        Ok(Self {
            width,
            height,
            title_bar,
            animation,
            node_count: nodes.len() as u8,
            nodes: owned_nodes,
        })
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes[..usize::from(self.node_count)]
    }

    pub fn node_rect(&self, index: usize, frame: u64) -> Option<Rect> {
        let mut rect = self.nodes().get(index)?.rect;
        if index == usize::from(self.animation.node_index) {
            rect.x = self.animation.sample(frame)?;
        }
        Some(rect)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneResult {
    pub scene: Scene,
    pub exit: GraphExitRecord,
}

impl SceneResult {
    pub fn encode(self) -> [u8; RESULT_LEN] {
        let mut bytes = [0; RESULT_LEN];
        bytes[..8].copy_from_slice(RESULT_MAGIC);
        bytes[8..10].copy_from_slice(&self.scene.width.to_le_bytes());
        bytes[10..12].copy_from_slice(&self.scene.height.to_le_bytes());
        self.scene.title_bar.encode(&mut bytes[12..20]);
        bytes[20] = self.scene.node_count;
        self.scene.animation.encode(&mut bytes[24..44]);
        let exit = self.exit;
        bytes[48..50].copy_from_slice(&exit.status.to_le_bytes());
        bytes[50..52].copy_from_slice(&exit.error_code.to_le_bytes());
        bytes[52..56].copy_from_slice(&exit.last_node.to_le_bytes());
        bytes[56..64].copy_from_slice(&exit.executed_nodes.to_le_bytes());
        bytes[64..66].copy_from_slice(&exit.result_type.to_le_bytes());
        bytes[66..68].copy_from_slice(&exit.reserved0.to_le_bytes());
        bytes[68..72].copy_from_slice(&exit.reserved1.to_le_bytes());
        bytes[72..80].copy_from_slice(&exit.result_raw.to_le_bytes());
        for (record, node) in bytes[RESULT_HEADER_LEN..]
            .chunks_exact_mut(NODE_LEN)
            .zip(self.scene.nodes())
        {
            node.encode(record);
        }
        bytes
    }

    pub fn decode(bytes: &[u8; RESULT_LEN]) -> Result<Self, WindowError> {
        let count = usize::from(bytes[20]);
        if &bytes[..8] != RESULT_MAGIC
            || !zero(&bytes[21..24])
            || bytes[25] != 0
            || !zero(&bytes[44..48])
            || count > MAX_NODES
        {
            return Err(WindowError::Result);
        }
        let nodes_end = RESULT_HEADER_LEN + count * NODE_LEN;
        if !zero(&bytes[nodes_end..]) {
            return Err(WindowError::Result);
        }
        let exit = GraphExitRecord {
            status: read_u16(bytes, 48),
            error_code: read_u16(bytes, 50),
            last_node: read_u32(bytes, 52),
            executed_nodes: read_u64(bytes, 56),
            result_type: read_u16(bytes, 64),
            reserved0: read_u16(bytes, 66),
            reserved1: read_u32(bytes, 68),
            result_raw: read_u64(bytes, 72),
        };
        if !graph_completed(exit) {
            return Err(WindowError::Result);
        }
        let mut nodes = [Node::EMPTY; MAX_NODES];
        for (record, node) in bytes[RESULT_HEADER_LEN..nodes_end]
            .chunks_exact(NODE_LEN)
            .zip(&mut nodes)
        {
            if !zero(&record[13..16]) {
                return Err(WindowError::Result);
            }
            *node = Node::decode(record);
        }
        let scene = Scene::new(
            read_u16(bytes, 8),
            read_u16(bytes, 10),
            Rect::decode(&bytes[12..20]),
            &nodes[..count],
            Animation::decode(&bytes[24..44]),
        )
        .map_err(|_| WindowError::Result)?;
        Ok(Self { scene, exit })
    }
}

pub fn graph_completed(exit: GraphExitRecord) -> bool {
    exit.status == GRAPH_EXIT_OK
        && exit.error_code == 0
        && exit.result_type == GRAPH_RESULT_UNIT
        && exit.reserved0 == 0
        && exit.reserved1 == 0
        && exit.result_raw == 0
        && exit.executed_nodes <= INSTRUCTION_BUDGET
        && exit.executed_nodes == u64::from(exit.last_node) + 1
}

fn zero(bytes: &[u8]) -> bool {
    bytes.iter().all(|&byte| byte == 0)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
