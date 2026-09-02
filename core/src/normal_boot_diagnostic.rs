//! Opt-in framebuffer breadcrumbs for normal-boot hardware diagnosis.
//!
//! This is intentionally separate from the default normal boot path. It gives a
//! serial-less physical machine a visible "last reached" stage without changing
//! the existing COM1 marker contract.

#[cfg(feature = "normal-boot-diagnostic")]
use crate::{framebuffer, serial};
use pythos_shared::boot_protocol::PythFramebufferInfo;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InitErrorDiagnostic {
    Memory,
    InterruptsTimer,
    TaskProcess,
    Ring3,
    UserStacks,
    BlockDevice,
    ShellProgram,
    ShellAddressSpace,
    ShellBootstrap,
    DefaultServiceGraphPackage,
}

impl InitErrorDiagnostic {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Memory => "init memory",
            Self::InterruptsTimer => "init timer",
            Self::TaskProcess => "init task proc",
            Self::Ring3 => "init ring3",
            Self::UserStacks => "init user stacks",
            Self::BlockDevice => "init block dev",
            Self::ShellProgram => "init shell prog",
            Self::ShellAddressSpace => "init shell map",
            Self::ShellBootstrap => "init shell boot",
            Self::DefaultServiceGraphPackage => "init svc graph",
        }
    }

    #[cfg(test)]
    pub(crate) const fn marker_suffix(self) -> &'static str {
        match self {
            Self::Memory => "MEMORY",
            Self::InterruptsTimer => "INTERRUPTS_TIMER",
            Self::TaskProcess => "TASK_PROCESS",
            Self::Ring3 => "RING3",
            Self::UserStacks => "USER_STACKS",
            Self::BlockDevice => "BLOCK_DEVICE",
            Self::ShellProgram => "SHELL_PROGRAM",
            Self::ShellAddressSpace => "SHELL_ADDRESS_SPACE",
            Self::ShellBootstrap => "SHELL_BOOTSTRAP",
            Self::DefaultServiceGraphPackage => "DEFAULT_SERVICE_GRAPH_PACKAGE",
        }
    }

    #[cfg(all(not(test), feature = "normal-boot-diagnostic"))]
    const fn marker(self) -> &'static str {
        match self {
            Self::Memory => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:MEMORY",
            Self::InterruptsTimer => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:INTERRUPTS_TIMER",
            Self::TaskProcess => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:TASK_PROCESS",
            Self::Ring3 => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:RING3",
            Self::UserStacks => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:USER_STACKS",
            Self::BlockDevice => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:BLOCK_DEVICE",
            Self::ShellProgram => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:SHELL_PROGRAM",
            Self::ShellAddressSpace => {
                "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:SHELL_ADDRESS_SPACE"
            }
            Self::ShellBootstrap => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:SHELL_BOOTSTRAP",
            Self::DefaultServiceGraphPackage => {
                "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR:DEFAULT_SERVICE_GRAPH_PACKAGE"
            }
        }
    }
}

#[cfg(not(test))]
impl From<crate::normal_init::NormalInitError> for InitErrorDiagnostic {
    fn from(error: crate::normal_init::NormalInitError) -> Self {
        match error {
            crate::normal_init::NormalInitError::Memory => Self::Memory,
            crate::normal_init::NormalInitError::InterruptsTimer => Self::InterruptsTimer,
            crate::normal_init::NormalInitError::TaskProcess => Self::TaskProcess,
            crate::normal_init::NormalInitError::Ring3 => Self::Ring3,
            crate::normal_init::NormalInitError::UserStacks => Self::UserStacks,
            crate::normal_init::NormalInitError::BlockDevice => Self::BlockDevice,
            crate::normal_init::NormalInitError::ShellProgram => Self::ShellProgram,
            crate::normal_init::NormalInitError::ShellAddressSpace => Self::ShellAddressSpace,
            crate::normal_init::NormalInitError::ShellBootstrap => Self::ShellBootstrap,
            crate::normal_init::NormalInitError::DefaultServiceGraphPackage => {
                Self::DefaultServiceGraphPackage
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NormalBootDiagnosticStage {
    CoreReady,
    NormalEnter,
    LoadShell,
    ShellValid,
    BootPage,
    ProbeAhci,
    ProbeSdhci,
    KernelMap,
    ShellMap,
    SessionPkg,
    StewardPkg,
    ActivateRoot,
    KernelActive,
    Timer,
    TaskProcess,
    Syscall,
    UserStacks,
    BlockSelect,
    BlockReady,
    InitError,
    SubstrateReady,
    StoreRestore,
    StoreError,
    PkgRestore,
    PkgError,
    Services,
    ServiceError,
    ServicesReady,
    Com2,
    Cinematic,
    Launcher,
    Ps2Wait,
    KeyboardReady,
    KeyboardFailed,
    ShellProcess,
    ShellError,
    Bootstrap,
    Ring3Enter,
}

impl NormalBootDiagnosticStage {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::CoreReady => "00",
            Self::NormalEnter => "01",
            Self::LoadShell => "02",
            Self::ShellValid => "03",
            Self::BootPage => "04",
            Self::ProbeAhci => "05",
            Self::ProbeSdhci => "06",
            Self::KernelMap => "07",
            Self::ShellMap => "08",
            Self::SessionPkg => "09",
            Self::StewardPkg => "10",
            Self::ActivateRoot => "11",
            Self::KernelActive => "12",
            Self::Timer => "13",
            Self::TaskProcess => "14",
            Self::Syscall => "15",
            Self::UserStacks => "16",
            Self::BlockSelect => "17",
            Self::BlockReady => "18",
            Self::InitError => "19",
            Self::SubstrateReady => "20",
            Self::StoreRestore => "21",
            Self::StoreError => "22",
            Self::PkgRestore => "23",
            Self::PkgError => "24",
            Self::Services => "25",
            Self::ServiceError => "26",
            Self::ServicesReady => "27",
            Self::Com2 => "28",
            Self::Cinematic => "29",
            Self::Launcher => "30",
            Self::Ps2Wait => "31",
            Self::KeyboardReady => "32",
            Self::KeyboardFailed => "33",
            Self::ShellProcess => "34",
            Self::ShellError => "35",
            Self::Bootstrap => "36",
            Self::Ring3Enter => "37",
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::CoreReady => "core ready",
            Self::NormalEnter => "normal enter",
            Self::LoadShell => "load shell",
            Self::ShellValid => "shell valid",
            Self::BootPage => "boot page",
            Self::ProbeAhci => "probe ahci",
            Self::ProbeSdhci => "probe sdhci",
            Self::KernelMap => "kernel map",
            Self::ShellMap => "shell map",
            Self::SessionPkg => "session pkg",
            Self::StewardPkg => "steward pkg",
            Self::ActivateRoot => "activate root",
            Self::KernelActive => "kernel active",
            Self::Timer => "timer",
            Self::TaskProcess => "task process",
            Self::Syscall => "syscall",
            Self::UserStacks => "user stacks",
            Self::BlockSelect => "block select",
            Self::BlockReady => "block ready",
            Self::InitError => "init error",
            Self::SubstrateReady => "substrate ready",
            Self::StoreRestore => "store restore",
            Self::StoreError => "store error",
            Self::PkgRestore => "pkg restore",
            Self::PkgError => "pkg error",
            Self::Services => "pyth services",
            Self::ServiceError => "service error",
            Self::ServicesReady => "services ready",
            Self::Com2 => "com2",
            Self::Cinematic => "cinematic",
            Self::Launcher => "launcher",
            Self::Ps2Wait => "ps2 wait",
            Self::KeyboardReady => "keyboard ready",
            Self::KeyboardFailed => "keyboard failed",
            Self::ShellProcess => "shell process",
            Self::ShellError => "shell error",
            Self::Bootstrap => "bootstrap",
            Self::Ring3Enter => "ring3 enter",
        }
    }

    #[cfg(test)]
    const fn marker_suffix(self) -> &'static str {
        match self {
            Self::CoreReady => "CORE_READY",
            Self::NormalEnter => "NORMAL_ENTER",
            Self::LoadShell => "LOAD_SHELL",
            Self::ShellValid => "SHELL_VALID",
            Self::BootPage => "BOOT_PAGE",
            Self::ProbeAhci => "PROBE_AHCI",
            Self::ProbeSdhci => "PROBE_SDHCI",
            Self::KernelMap => "KERNEL_MAP",
            Self::ShellMap => "SHELL_MAP",
            Self::SessionPkg => "SESSION_PKG",
            Self::StewardPkg => "STEWARD_PKG",
            Self::ActivateRoot => "ACTIVATE_ROOT",
            Self::KernelActive => "KERNEL_ACTIVE",
            Self::Timer => "TIMER",
            Self::TaskProcess => "TASK_PROCESS",
            Self::Syscall => "SYSCALL",
            Self::UserStacks => "USER_STACKS",
            Self::BlockSelect => "BLOCK_SELECT",
            Self::BlockReady => "BLOCK_READY",
            Self::InitError => "INIT_ERROR",
            Self::SubstrateReady => "SUBSTRATE_READY",
            Self::StoreRestore => "STORE_RESTORE",
            Self::StoreError => "STORE_ERROR",
            Self::PkgRestore => "PKG_RESTORE",
            Self::PkgError => "PKG_ERROR",
            Self::Services => "SERVICES",
            Self::ServiceError => "SERVICE_ERROR",
            Self::ServicesReady => "SERVICES_READY",
            Self::Com2 => "COM2",
            Self::Cinematic => "CINEMATIC",
            Self::Launcher => "LAUNCHER",
            Self::Ps2Wait => "PS2_WAIT",
            Self::KeyboardReady => "KEYBOARD_READY",
            Self::KeyboardFailed => "KEYBOARD_FAILED",
            Self::ShellProcess => "SHELL_PROCESS",
            Self::ShellError => "SHELL_ERROR",
            Self::Bootstrap => "BOOTSTRAP",
            Self::Ring3Enter => "RING3_ENTER",
        }
    }

    #[cfg(all(not(test), feature = "normal-boot-diagnostic"))]
    const fn marker(self) -> &'static str {
        match self {
            Self::CoreReady => "PYTHOS:CORE:NORMAL_BOOT_DIAG:CORE_READY",
            Self::NormalEnter => "PYTHOS:CORE:NORMAL_BOOT_DIAG:NORMAL_ENTER",
            Self::LoadShell => "PYTHOS:CORE:NORMAL_BOOT_DIAG:LOAD_SHELL",
            Self::ShellValid => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SHELL_VALID",
            Self::BootPage => "PYTHOS:CORE:NORMAL_BOOT_DIAG:BOOT_PAGE",
            Self::ProbeAhci => "PYTHOS:CORE:NORMAL_BOOT_DIAG:PROBE_AHCI",
            Self::ProbeSdhci => "PYTHOS:CORE:NORMAL_BOOT_DIAG:PROBE_SDHCI",
            Self::KernelMap => "PYTHOS:CORE:NORMAL_BOOT_DIAG:KERNEL_MAP",
            Self::ShellMap => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SHELL_MAP",
            Self::SessionPkg => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SESSION_PKG",
            Self::StewardPkg => "PYTHOS:CORE:NORMAL_BOOT_DIAG:STEWARD_PKG",
            Self::ActivateRoot => "PYTHOS:CORE:NORMAL_BOOT_DIAG:ACTIVATE_ROOT",
            Self::KernelActive => "PYTHOS:CORE:NORMAL_BOOT_DIAG:KERNEL_ACTIVE",
            Self::Timer => "PYTHOS:CORE:NORMAL_BOOT_DIAG:TIMER",
            Self::TaskProcess => "PYTHOS:CORE:NORMAL_BOOT_DIAG:TASK_PROCESS",
            Self::Syscall => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SYSCALL",
            Self::UserStacks => "PYTHOS:CORE:NORMAL_BOOT_DIAG:USER_STACKS",
            Self::BlockSelect => "PYTHOS:CORE:NORMAL_BOOT_DIAG:BLOCK_SELECT",
            Self::BlockReady => "PYTHOS:CORE:NORMAL_BOOT_DIAG:BLOCK_READY",
            Self::InitError => "PYTHOS:CORE:NORMAL_BOOT_DIAG:INIT_ERROR",
            Self::SubstrateReady => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SUBSTRATE_READY",
            Self::StoreRestore => "PYTHOS:CORE:NORMAL_BOOT_DIAG:STORE_RESTORE",
            Self::StoreError => "PYTHOS:CORE:NORMAL_BOOT_DIAG:STORE_ERROR",
            Self::PkgRestore => "PYTHOS:CORE:NORMAL_BOOT_DIAG:PKG_RESTORE",
            Self::PkgError => "PYTHOS:CORE:NORMAL_BOOT_DIAG:PKG_ERROR",
            Self::Services => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SERVICES",
            Self::ServiceError => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SERVICE_ERROR",
            Self::ServicesReady => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SERVICES_READY",
            Self::Com2 => "PYTHOS:CORE:NORMAL_BOOT_DIAG:COM2",
            Self::Cinematic => "PYTHOS:CORE:NORMAL_BOOT_DIAG:CINEMATIC",
            Self::Launcher => "PYTHOS:CORE:NORMAL_BOOT_DIAG:LAUNCHER",
            Self::Ps2Wait => "PYTHOS:CORE:NORMAL_BOOT_DIAG:PS2_WAIT",
            Self::KeyboardReady => "PYTHOS:CORE:NORMAL_BOOT_DIAG:KEYBOARD_READY",
            Self::KeyboardFailed => "PYTHOS:CORE:NORMAL_BOOT_DIAG:KEYBOARD_FAILED",
            Self::ShellProcess => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SHELL_PROCESS",
            Self::ShellError => "PYTHOS:CORE:NORMAL_BOOT_DIAG:SHELL_ERROR",
            Self::Bootstrap => "PYTHOS:CORE:NORMAL_BOOT_DIAG:BOOTSTRAP",
            Self::Ring3Enter => "PYTHOS:CORE:NORMAL_BOOT_DIAG:RING3_ENTER",
        }
    }
}

#[cfg(test)]
const ALL_STAGES: &[NormalBootDiagnosticStage] = &[
    NormalBootDiagnosticStage::CoreReady,
    NormalBootDiagnosticStage::NormalEnter,
    NormalBootDiagnosticStage::LoadShell,
    NormalBootDiagnosticStage::ShellValid,
    NormalBootDiagnosticStage::BootPage,
    NormalBootDiagnosticStage::ProbeAhci,
    NormalBootDiagnosticStage::ProbeSdhci,
    NormalBootDiagnosticStage::KernelMap,
    NormalBootDiagnosticStage::ShellMap,
    NormalBootDiagnosticStage::SessionPkg,
    NormalBootDiagnosticStage::StewardPkg,
    NormalBootDiagnosticStage::ActivateRoot,
    NormalBootDiagnosticStage::KernelActive,
    NormalBootDiagnosticStage::Timer,
    NormalBootDiagnosticStage::TaskProcess,
    NormalBootDiagnosticStage::Syscall,
    NormalBootDiagnosticStage::UserStacks,
    NormalBootDiagnosticStage::BlockSelect,
    NormalBootDiagnosticStage::BlockReady,
    NormalBootDiagnosticStage::InitError,
    NormalBootDiagnosticStage::SubstrateReady,
    NormalBootDiagnosticStage::StoreRestore,
    NormalBootDiagnosticStage::StoreError,
    NormalBootDiagnosticStage::PkgRestore,
    NormalBootDiagnosticStage::PkgError,
    NormalBootDiagnosticStage::Services,
    NormalBootDiagnosticStage::ServiceError,
    NormalBootDiagnosticStage::ServicesReady,
    NormalBootDiagnosticStage::Com2,
    NormalBootDiagnosticStage::Cinematic,
    NormalBootDiagnosticStage::Launcher,
    NormalBootDiagnosticStage::Ps2Wait,
    NormalBootDiagnosticStage::KeyboardReady,
    NormalBootDiagnosticStage::KeyboardFailed,
    NormalBootDiagnosticStage::ShellProcess,
    NormalBootDiagnosticStage::ShellError,
    NormalBootDiagnosticStage::Bootstrap,
    NormalBootDiagnosticStage::Ring3Enter,
];

#[cfg(all(not(test), feature = "normal-boot-diagnostic"))]
pub(crate) fn report(framebuffer: &PythFramebufferInfo, stage: NormalBootDiagnosticStage) {
    serial::write_line(stage.marker());
    let _ = framebuffer::render_normal_boot_diagnostic(framebuffer, stage.code(), stage.label());
}

#[cfg(all(not(test), feature = "normal-boot-diagnostic"))]
pub(crate) fn report_init_error(framebuffer: &PythFramebufferInfo, error: InitErrorDiagnostic) {
    let stage = NormalBootDiagnosticStage::InitError;
    serial::write_line(stage.marker());
    serial::write_line(error.marker());
    let _ = framebuffer::render_normal_boot_diagnostic(framebuffer, stage.code(), error.label());
}

#[cfg(any(test, not(feature = "normal-boot-diagnostic")))]
pub(crate) fn report_init_error(_framebuffer: &PythFramebufferInfo, _error: InitErrorDiagnostic) {}

#[cfg(any(test, not(feature = "normal-boot-diagnostic")))]
pub(crate) fn report(_framebuffer: &PythFramebufferInfo, _stage: NormalBootDiagnosticStage) {}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_INIT_ERRORS: &[InitErrorDiagnostic] = &[
        InitErrorDiagnostic::Memory,
        InitErrorDiagnostic::InterruptsTimer,
        InitErrorDiagnostic::TaskProcess,
        InitErrorDiagnostic::Ring3,
        InitErrorDiagnostic::UserStacks,
        InitErrorDiagnostic::BlockDevice,
        InitErrorDiagnostic::ShellProgram,
        InitErrorDiagnostic::ShellAddressSpace,
        InitErrorDiagnostic::ShellBootstrap,
        InitErrorDiagnostic::DefaultServiceGraphPackage,
    ];

    #[test]
    fn stage_codes_are_two_digits() {
        for stage in ALL_STAGES {
            let code = stage.code();
            assert_eq!(code.len(), 2);
            assert!(code.bytes().all(|byte| byte.is_ascii_digit()));
        }
    }

    #[test]
    fn stage_labels_fit_boot_font_subset() {
        for stage in ALL_STAGES {
            for byte in stage.label().bytes() {
                assert!(
                    crate::font::glyph(byte).is_some(),
                    "unsupported glyph {byte:?} in {:?}",
                    stage
                );
            }
        }
    }

    #[test]
    fn marker_suffixes_are_stable_ascii() {
        for stage in ALL_STAGES {
            let suffix = stage.marker_suffix();
            assert!(suffix.bytes().all(|byte| {
                byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
            }));
        }
    }

    #[test]
    fn init_error_labels_fit_boot_font_subset() {
        for error in ALL_INIT_ERRORS {
            let label = error.label();
            assert!(label.starts_with("init "));
            for byte in label.bytes() {
                assert!(
                    crate::font::glyph(byte).is_some(),
                    "unsupported glyph {byte:?} in {error:?}"
                );
            }
        }
    }

    #[test]
    fn init_error_marker_suffixes_are_stable_ascii() {
        for error in ALL_INIT_ERRORS {
            let suffix = error.marker_suffix();
            assert!(!suffix.is_empty());
            assert!(suffix.bytes().all(|byte| {
                byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
            }));
        }
    }
}
