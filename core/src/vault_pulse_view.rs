//! Project observations of boot, admitted instructions, and committed state.

use core::arch::x86_64::{__cpuid, _rdtsc};
use pythos_shared::{
    boot_protocol::PythFramebufferInfo,
    vault_pulse::{EventKind, Instruction, MAX_PROGRAM_ADDRESSES, Program, Status},
};

use crate::{framebuffer::vault_view, serial, vault_pulse_store::Checkpoint};

const MAX_LINKS: usize = MAX_PROGRAM_ADDRESSES * 2;
const FRAME_CYCLES: u64 = 600_000_000;

#[derive(Clone, Copy)]
pub(crate) struct Node {
    pub(crate) name: &'static str,
    pub(crate) address: Option<u8>,
}

#[derive(Clone, Copy)]
pub(crate) struct Link {
    pub(crate) from: usize,
    pub(crate) to: usize,
    pub(crate) alternate: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct State {
    pub(crate) generation: u64,
    pub(crate) x: u64,
    pub(crate) goal: u64,
    pub(crate) address: u8,
    pub(crate) status: Status,
}

pub(crate) struct Scene<'a> {
    pub(crate) nodes: &'a [Node],
    pub(crate) links: &'a [Link],
    pub(crate) state: Option<State>,
    pub(crate) message: &'a str,
    pub(crate) detail: &'a str,
    pub(crate) blocked: bool,
}

pub(crate) struct Graph {
    nodes: [Node; MAX_PROGRAM_ADDRESSES],
    links: [Link; MAX_LINKS],
    node_count: usize,
    link_count: usize,
    program: Option<Program>,
    state: Option<State>,
}

impl Graph {
    pub(crate) const fn new() -> Self {
        Self {
            nodes: [Node {
                name: "",
                address: None,
            }; MAX_PROGRAM_ADDRESSES],
            links: [Link {
                from: 0,
                to: 0,
                alternate: false,
            }; MAX_LINKS],
            node_count: 0,
            link_count: 0,
            program: None,
            state: None,
        }
    }

    pub(crate) fn boot_node(
        &mut self,
        fb: &PythFramebufferInfo,
        marker: &str,
        name: &'static str,
        message: &str,
    ) -> Result<(), ()> {
        self.nodes[self.node_count] = Node {
            name,
            address: None,
        };
        self.node_count += 1;
        self.draw(fb, message, "", marker == "STORAGE")?;
        mark("NODE ");
        serial::write_line(marker);
        dwell(FRAME_CYCLES);
        Ok(())
    }

    pub(crate) fn boot_links(
        &mut self,
        fb: &PythFramebufferInfo,
        message: &str,
        detail: &str,
        blocked: bool,
    ) -> Result<(), ()> {
        for to in 1..self.node_count {
            self.add_link(to - 1, to, false);
        }
        self.draw(fb, message, detail, blocked)?;
        mark("BOOT_LINKS");
        serial::write_line("");
        dwell(FRAME_CYCLES);
        Ok(())
    }

    pub(crate) fn program(&mut self, fb: &PythFramebufferInfo, program: Program) -> Result<(), ()> {
        self.node_count = 0;
        self.link_count = 0;
        self.program = Some(program);
        self.state = None;

        let message = "Reading saved instructions";
        let mut links = [None; MAX_PROGRAM_ADDRESSES];
        for (index, link) in links.iter_mut().enumerate() {
            let address = index as u8;
            let Some(instruction) = program.instruction(address) else {
                break;
            };
            let (name, next, alternate) = match instruction {
                Instruction::Add { next } => ("Add event value", next, None),
                Instruction::Branch { on, yes, no } => (branch_name(on), yes, Some(no)),
                Instruction::Wait { for_kind, next } => (wait_name(for_kind), next, None),
            };
            *link = Some((next, alternate));
            self.nodes[index] = Node {
                name,
                address: Some(address),
            };
            self.node_count += 1;
            self.draw(fb, message, "", false)?;
            mark("ADDRESS ");
            serial::write_dec_u64_value(u64::from(address));
            serial::write_line("");
            dwell(FRAME_CYCLES);
        }

        for (from, (next, alternate)) in links.into_iter().flatten().enumerate() {
            self.add_link(from, usize::from(next), false);
            if let Some(no) = alternate {
                self.add_link(from, usize::from(no), true);
            }
        }
        self.draw(fb, "Instruction links ready", "", false)?;
        for link in &self.links[..self.link_count] {
            mark("LINK ");
            serial::write_dec_u64_value(link.from as u64);
            serial::write_str(" ");
            serial::write_dec_u64_value(link.to as u64);
            serial::write_line("");
        }
        dwell(FRAME_CYCLES);
        Ok(())
    }

    pub(crate) fn checkpoint(
        &mut self,
        fb: &PythFramebufferInfo,
        checkpoint: Checkpoint,
        origin: &str,
        status: &str,
    ) -> Result<(), ()> {
        let program = self.program.ok_or(())?;
        self.state = Some(State {
            generation: checkpoint.generation,
            x: checkpoint.x,
            goal: program.goal(),
            address: checkpoint.cursor,
            status: checkpoint.status,
        });
        let detail = match (checkpoint.status, checkpoint.active) {
            (Status::Waiting, _) => wait_name(program.wait_kind(checkpoint.cursor).ok_or(())?),
            (_, Some(event)) => event_name(event.kind),
            _ => "",
        };
        let message = match (origin, checkpoint.status) {
            ("restored", _) => "Checkpoint restored",
            (_, Status::Waiting) => "Waiting for an event",
            (_, Status::Finished) => "Goal reached",
            ("admitted", _) => "Task admitted",
            _ => "Pulse committed",
        };
        self.draw(fb, message, detail, false)?;
        mark("STATE ");
        serial::write_str(origin);
        number(" generation:", checkpoint.generation);
        number(" x:", checkpoint.x);
        number(" cursor:", u64::from(checkpoint.cursor));
        serial::write_str(" status:");
        serial::write_line(status);
        dwell(FRAME_CYCLES);
        Ok(())
    }

    pub(crate) fn stop(
        &mut self,
        fb: &PythFramebufferInfo,
        code: &str,
        message: &str,
        detail: &str,
    ) -> Result<(), ()> {
        let blocked = !matches!(code, "IDLE" | "YIELDED");
        if matches!(code, "VIRTIO_BLOCK" | "PROOF_DEVICE" | "PROOF_IMAGE") {
            self.boot_node(fb, "STORAGE", "Virtio storage", message)?;
            self.boot_links(fb, message, detail, true)?;
        } else {
            self.draw(fb, message, detail, blocked)?;
        }
        mark("STOP ");
        serial::write_line(code);
        Ok(())
    }

    fn add_link(&mut self, from: usize, to: usize, alternate: bool) {
        self.links[self.link_count] = Link {
            from,
            to,
            alternate,
        };
        self.link_count += 1;
    }

    fn draw(
        &self,
        fb: &PythFramebufferInfo,
        message: &str,
        detail: &str,
        blocked: bool,
    ) -> Result<(), ()> {
        vault_view::render(
            fb,
            Scene {
                nodes: &self.nodes[..self.node_count],
                links: &self.links[..self.link_count],
                state: self.state,
                message,
                detail,
                blocked,
            },
        )
    }
}

fn mark(marker: &str) {
    serial::write_str("PYTHOS:VAULT_VIEW:");
    serial::write_str(marker);
}

fn number(label: &str, value: u64) {
    serial::write_str(label);
    serial::write_dec_u64_value(value);
}

fn event_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Task => "Task event",
        EventKind::Message => "Message event",
        EventKind::System => "System event",
    }
}

fn branch_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Task => "Route task",
        EventKind::Message => "Route message",
        EventKind::System => "Route system event",
    }
}

fn wait_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Task => "Wait for a task",
        EventKind::Message => "Wait for a message",
        EventKind::System => "Wait for a system event",
    }
}

pub(crate) fn hold_terminal() {
    dwell(3 * FRAME_CYCLES);
}

fn dwell(cycles: u64) {
    // SAFETY: the loader enters PythCore on a single x86-64 CPU with native
    // CPUID support and ring-0 permissions. Leaf 1 is valid for this boot.
    // The read owns no pointers, alignment, mapping, or borrowed lifetime.
    // Unsupported timing hardware skips the dwell instead of reading its TSC.
    let features = unsafe { __cpuid(1) };
    if features.edx & (1 << 4) == 0 {
        return;
    }
    let start = read_tsc();
    // This display cadence is approximate. Give up if the counter stalls.
    for _ in 0..20_000_000 {
        if read_tsc().wrapping_sub(start) >= cycles {
            break;
        }
        core::hint::spin_loop();
    }
}

fn read_tsc() -> u64 {
    // SAFETY: dwell checks the CPU's TSC feature bit before every animation.
    // This single-core ring-0 instruction has no pointers, alignment, mapped
    // length, or borrowed lifetime. Reading on a CPU without that bit could
    // fault, and the result is used only for the bounded presentation delay.
    unsafe { _rdtsc() }
}
