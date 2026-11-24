//! Opt-in graph projection and the text diagnostic fallback.

use crate::{framebuffer, serial, vault_pulse_store::Checkpoint};
#[cfg(feature = "vault-pulse-view")]
use crate::{vault_pulse_probe::status_name, vault_pulse_view::Graph};
use pythos_shared::{boot_protocol::PythFramebufferInfo, vault_pulse::Program};

#[derive(Clone, Copy)]
pub(super) enum BootStage {
    Core,
    Worker,
    Memory,
    Vault,
}

impl BootStage {
    fn message(self) -> &'static str {
        match self {
            Self::Core => "Starting vault runtime",
            Self::Worker => "Mapping kernel memory",
            Self::Memory => "Checking Virtio storage",
            Self::Vault => "Recovering vault checkpoint",
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Origin {
    Admitted,
    Restored,
}

pub(super) struct BootDisplay<'a> {
    framebuffer: &'a PythFramebufferInfo,
    #[cfg(feature = "vault-pulse-view")]
    graph: Option<Graph>,
}

impl<'a> BootDisplay<'a> {
    pub(super) fn new(framebuffer: &'a PythFramebufferInfo) -> Self {
        Self {
            framebuffer,
            #[cfg(feature = "vault-pulse-view")]
            graph: Some(Graph::new()),
        }
    }

    pub(super) fn stage(&mut self, stage: BootStage) {
        #[cfg(feature = "vault-pulse-view")]
        {
            let (marker, label) = match stage {
                BootStage::Core => ("CORE", "PythCore"),
                BootStage::Worker => ("WORKER", "Vault worker"),
                BootStage::Memory => ("MEMORY", "Memory mappings"),
                BootStage::Vault => ("VAULT", "Marked vault disk"),
            };
            if self.project(|graph, fb| {
                graph.boot_node(fb, marker, label, stage.message())?;
                if let BootStage::Vault = stage {
                    graph.boot_links(fb, stage.message(), "", false)?;
                }
                Ok(())
            }) {
                return;
            }
        }
        self.text(stage.message(), "");
    }

    pub(super) fn prepare_worker(&self) {
        #[cfg(feature = "vault-pulse-view")]
        if self.graph.is_some() {
            return;
        }
        self.text("Preparing vault worker", "");
    }

    pub(super) fn program(&mut self, _program: Program, _checkpoint: Checkpoint, _origin: Origin) {
        #[cfg(feature = "vault-pulse-view")]
        self.project(|graph, fb| {
            graph.program(fb, _program)?;
            let origin = match _origin {
                Origin::Admitted => "admitted",
                Origin::Restored => "restored",
            };
            graph.checkpoint(fb, _checkpoint, origin, status_name(_checkpoint.status))
        });
    }

    pub(super) fn pulse(&mut self, _checkpoint: Checkpoint) {
        #[cfg(feature = "vault-pulse-view")]
        self.project(|graph, fb| {
            graph.checkpoint(fb, _checkpoint, "pulse", status_name(_checkpoint.status))
        });
    }

    pub(super) fn stop(&mut self, _code: &str, message: &str, detail: &str) {
        #[cfg(feature = "vault-pulse-view")]
        if self.project(|graph, fb| graph.stop(fb, _code, message, detail)) {
            return;
        }
        self.text(message, detail);
    }

    pub(super) fn hold_terminal(&self) {
        #[cfg(feature = "vault-pulse-view")]
        if self.graph.is_some() {
            crate::vault_pulse_view::hold_terminal();
        }
    }

    #[cfg(feature = "vault-pulse-view")]
    fn project(
        &mut self,
        draw: impl FnOnce(&mut Graph, &PythFramebufferInfo) -> Result<(), ()>,
    ) -> bool {
        let Some(graph) = &mut self.graph else {
            return false;
        };
        if draw(graph, self.framebuffer).is_ok() {
            return true;
        }
        self.graph = None;
        serial::write_line("PYTHOS:VAULT_VIEW:RENDER_FAILED");
        false
    }

    fn text(&self, message: &str, detail: &str) {
        if framebuffer::render_hardware_probe_lines(
            self.framebuffer,
            &["PythOS vault", message, detail],
        )
        .is_err()
        {
            serial::write_line("PYTHOS:VAULT_SCREEN:RENDER_FAILED");
        }
    }
}
