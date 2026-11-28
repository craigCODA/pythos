#![no_std]

use pythos_shared::{
    capability_abi::PackedCapability,
    graph_window::{
        Animation, INSTRUCTION_BUDGET, KEYFRAME_COUNT, Keyframe, MAX_NODES, Node, Rect, Scene,
        SceneResult, WindowError, graph_completed, validate_graph,
    },
    pyth_runtime_abi::{HostCallResult, MAX_PYTH_GRAPH_IMPORTS},
    pyth_tig::{format::MAX_RUNTIME_VALUES, verify::VerifiedGraph},
};
use pythos_user_pyth_runtime::{
    interpreter::{Host, HostError, Interpreter},
    value::Value,
};

const MAX_OBJECTS: usize = MAX_NODES + 2;
const MAX_REVISION_BYTES: usize = 64;
// Invocation-local object handles occupy a separate slot range from the imported workspace.
const LOCAL_CAPABILITY_BASE: u32 = 1 << 31;

pub struct Scratch {
    values: [Option<Value>; MAX_RUNTIME_VALUES],
    host_results: [Option<HostCallResult>; MAX_RUNTIME_VALUES],
}

impl Scratch {
    pub const fn new() -> Self {
        Self {
            values: [None; MAX_RUNTIME_VALUES],
            host_results: [None; MAX_RUNTIME_VALUES],
        }
    }
}

impl Default for Scratch {
    fn default() -> Self {
        Self::new()
    }
}

pub fn evaluate(
    verified: VerifiedGraph<'_>,
    workspace: PackedCapability,
    scratch: &mut Scratch,
) -> Result<SceneResult, WindowError> {
    validate_graph(&verified)?;
    if workspace.generation() == 0 || workspace.slot() >= LOCAL_CAPABILITY_BASE {
        return Err(WindowError::Invocation);
    }
    let mut imports = [PackedCapability::from_raw(0); MAX_PYTH_GRAPH_IMPORTS];
    imports[0] = workspace;
    let mut host = WindowHost {
        workspace,
        objects: [None; MAX_OBJECTS],
        count: 0,
    };
    let exit = Interpreter::new(
        verified,
        &imports,
        INSTRUCTION_BUDGET,
        &mut scratch.values,
        &mut scratch.host_results,
    )
    .execute(&mut host);
    if !graph_completed(exit) || exit.executed_nodes != verified.package().nodes().len() as u64 {
        return Err(WindowError::GraphExecution);
    }
    Ok(SceneResult {
        scene: host.scene()?,
        exit,
    })
}

struct WindowHost {
    workspace: PackedCapability,
    objects: [Option<WindowObject>; MAX_OBJECTS],
    count: usize,
}

#[derive(Clone, Copy)]
struct WindowObject {
    capability: PackedCapability,
    revision: Option<Revision>,
}

#[derive(Clone, Copy)]
enum Revision {
    Window {
        width: u16,
        height: u16,
        title_bar: Rect,
    },
    Node(Node),
    Animation(Animation),
}

impl WindowHost {
    fn scene(&self) -> Result<Scene, WindowError> {
        let Some(Revision::Window {
            width,
            height,
            title_bar,
        }) = self.objects[0].and_then(|object| object.revision)
        else {
            return Err(WindowError::Scene);
        };
        let last = self.count.checked_sub(1).ok_or(WindowError::Scene)?;
        let Some(Revision::Animation(animation)) =
            self.objects[last].and_then(|object| object.revision)
        else {
            return Err(WindowError::Scene);
        };
        let mut nodes = [Node::EMPTY; MAX_NODES];
        for (node, object) in nodes.iter_mut().zip(&self.objects[1..last]) {
            let Some(Revision::Node(value)) = object.and_then(|object| object.revision) else {
                return Err(WindowError::Scene);
            };
            *node = value;
        }
        Scene::new(width, height, title_bar, &nodes[..last - 1], animation)
    }
}

impl Host for WindowHost {
    fn object_create(
        &mut self,
        capability: PackedCapability,
        kind: &[u8],
    ) -> Result<HostCallResult, HostError> {
        if capability != self.workspace || kind != b"package-defined" {
            return Err(HostError::Denied);
        }
        let slot = self.objects.get_mut(self.count).ok_or(HostError::Denied)?;
        let object_capability =
            PackedCapability::from_parts(LOCAL_CAPABILITY_BASE + self.count as u32, 1);
        *slot = Some(WindowObject {
            capability: object_capability,
            revision: None,
        });
        self.count += 1;
        let mut result = HostCallResult::empty(0);
        result.object_id = self.count as u64;
        result.revision = 1;
        result.capability = object_capability;
        Ok(result)
    }

    fn object_revise(
        &mut self,
        capability: PackedCapability,
        object_id: u64,
        bytes: &[u8],
    ) -> Result<HostCallResult, HostError> {
        let slot = object_id
            .checked_sub(1)
            .and_then(|slot| usize::try_from(slot).ok())
            .ok_or(HostError::Denied)?;
        let object = self
            .objects
            .get_mut(slot)
            .and_then(Option::as_mut)
            .ok_or(HostError::Denied)?;
        if object.capability != capability || object.revision.is_some() {
            return Err(HostError::Denied);
        }
        object.revision = Some(decode_revision(bytes).ok_or(HostError::Denied)?);
        let mut result = HostCallResult::empty(0);
        result.object_id = object_id;
        result.revision = 2;
        Ok(result)
    }

    fn system_log(&mut self, _: PackedCapability, _: &[u8]) -> Result<(), HostError> {
        Err(HostError::Denied)
    }

    fn object_query(&mut self, _: PackedCapability, _: &[u8]) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn object_inspect(&mut self, _: PackedCapability, _: u64) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn object_history(&mut self, _: PackedCapability, _: u64) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn task_context(&mut self, _: PackedCapability) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn task_propose(&mut self, _: PackedCapability, _: u64, _: u64) -> Result<(), HostError> {
        Err(HostError::Denied)
    }

    fn command_read(&mut self, _: PackedCapability) -> Result<HostCallResult, HostError> {
        Err(HostError::Denied)
    }

    fn command_result_emit(
        &mut self,
        _: PackedCapability,
        _: u16,
        _: &[u8],
    ) -> Result<(), HostError> {
        Err(HostError::Denied)
    }
}

fn decode_revision(bytes: &[u8]) -> Option<Revision> {
    if bytes.len() > MAX_REVISION_BYTES {
        return None;
    }
    let mut fields = core::str::from_utf8(bytes).ok()?.split('|');
    let revision = match fields.next()? {
        "window" => Revision::Window {
            width: number(fields.next()?)?,
            height: number(fields.next()?)?,
            title_bar: rectangle(&mut fields)?,
        },
        "rect" => Revision::Node(Node::rectangle(
            rectangle(&mut fields)?,
            color(fields.next()?)?,
        )),
        "text" => Revision::Node(
            Node::label(
                rectangle(&mut fields)?,
                color(fields.next()?)?,
                fields.next()?.as_bytes(),
            )
            .ok()?,
        ),
        "horizontal" => {
            let node_index = u8::try_from(number(fields.next()?)?).ok()?;
            let period_frames = number(fields.next()?)?;
            let mut keyframes = [Keyframe { frame: 0, x: 0 }; KEYFRAME_COUNT];
            for keyframe in &mut keyframes {
                keyframe.frame = number(fields.next()?)?;
                keyframe.x = number(fields.next()?)?;
            }
            Revision::Animation(Animation {
                node_index,
                period_frames,
                keyframes,
            })
        }
        _ => return None,
    };
    fields.next().is_none().then_some(revision)
}

fn rectangle(fields: &mut core::str::Split<'_, char>) -> Option<Rect> {
    Some(Rect {
        x: number(fields.next()?)?,
        y: number(fields.next()?)?,
        width: number(fields.next()?)?,
        height: number(fields.next()?)?,
    })
}

fn number(field: &str) -> Option<u16> {
    if field.is_empty()
        || (field.len() > 1 && field.starts_with('0'))
        || !field.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    field.parse().ok()
}

fn color(field: &str) -> Option<u32> {
    if field.len() != 6
        || !field
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'A'..=b'F'))
    {
        return None;
    }
    u32::from_str_radix(field, 16).ok()
}
