pub const SESSION_MANAGER_READY_MARKER: &str = "PYTHOS:PYTHTIG:SESSION_MANAGER_READY";
pub const TASK_STEWARD_READY_MARKER: &str = "PYTHOS:PYTHTIG:TASK_STEWARD_READY";
pub const DEFAULT_SERVICES_READY_MARKER: &str = "PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY";
#[allow(dead_code)]
pub const SERVICE_FAULT_CONTAINED_MARKER: &str = "PYTHOS:PYTHTIG:SERVICE_FAULT_CONTAINED";
#[allow(dead_code)]
pub const SESSION_MANAGER_FAULT_CONTAINED_MARKER: &str =
    "PYTHOS:PYTHTIG:SERVICE_FAULT_CONTAINED service:session-manager";
#[allow(dead_code)]
pub const RECOVERY_SHELL_ENTER_MARKER: &str = "PYTHOS:PYTHTIG:RECOVERY_SHELL_ENTER";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalProgram {
    PythServices,
    LegacyShell,
}

pub const NORMAL_PROGRAM: NormalProgram = normal_program_for_features(
    cfg!(feature = "legacy-shell"),
    cfg!(feature = "pyth-tig-default"),
);

pub const fn normal_program() -> NormalProgram {
    NORMAL_PROGRAM
}

pub const fn normal_program_for_features(
    legacy_shell: bool,
    pyth_tig_default: bool,
) -> NormalProgram {
    if legacy_shell {
        NormalProgram::LegacyShell
    } else if pyth_tig_default {
        NormalProgram::PythServices
    } else {
        NormalProgram::LegacyShell
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServiceKind {
    SessionManager,
    TaskSteward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphExitStatus {
    Ok,
    Fault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorAction {
    RelaunchSessionManager,
    RelaunchTaskSteward,
    EnterRecoveryShell,
    Halt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PythServiceSupervisor {
    recovery_shell_available: bool,
    session_manager_faulted: bool,
    task_steward_faulted: bool,
    action: SupervisorAction,
}

impl PythServiceSupervisor {
    pub const fn new(recovery_shell_available: bool) -> Self {
        Self {
            recovery_shell_available,
            session_manager_faulted: false,
            task_steward_faulted: false,
            action: SupervisorAction::RelaunchSessionManager,
        }
    }

    #[cfg(test)]
    pub const fn new_for_test() -> Self {
        Self::new(true)
    }

    pub fn record_exit(&mut self, service: ServiceKind, status: GraphExitStatus) {
        match (service, status) {
            (ServiceKind::SessionManager, GraphExitStatus::Ok) => {
                self.session_manager_faulted = false;
                self.action = SupervisorAction::RelaunchSessionManager;
            }
            (ServiceKind::TaskSteward, GraphExitStatus::Ok) => {
                self.task_steward_faulted = false;
                self.action = SupervisorAction::RelaunchTaskSteward;
            }
            (ServiceKind::SessionManager, GraphExitStatus::Fault) => {
                self.session_manager_faulted = true;
                self.action = self.fault_action();
            }
            (ServiceKind::TaskSteward, GraphExitStatus::Fault) => {
                self.task_steward_faulted = true;
                self.action = self.fault_action();
            }
        }
    }

    pub const fn next_action(&self) -> SupervisorAction {
        self.action
    }

    pub const fn service_faulted(&self, service: ServiceKind) -> bool {
        match service {
            ServiceKind::SessionManager => self.session_manager_faulted,
            ServiceKind::TaskSteward => self.task_steward_faulted,
        }
    }

    const fn fault_action(&self) -> SupervisorAction {
        if self.recovery_shell_available {
            SupervisorAction::EnterRecoveryShell
        } else {
            SupervisorAction::Halt
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_normal_boot_selects_pyth_services_and_legacy_feature_selects_shell() {
        assert_eq!(
            normal_program_for_features(false, true),
            NormalProgram::PythServices
        );
        assert_eq!(
            normal_program_for_features(true, false),
            NormalProgram::LegacyShell
        );
    }

    #[test]
    fn supervisor_restarts_completed_bounded_service_but_not_fault_loop() {
        let mut supervisor = PythServiceSupervisor::new_for_test();
        supervisor.record_exit(ServiceKind::SessionManager, GraphExitStatus::Ok);
        assert_eq!(
            supervisor.next_action(),
            SupervisorAction::RelaunchSessionManager
        );
        supervisor.record_exit(ServiceKind::SessionManager, GraphExitStatus::Fault);
        assert_eq!(
            supervisor.next_action(),
            SupervisorAction::EnterRecoveryShell
        );
    }

    #[test]
    fn supervisor_tracks_task_steward_exit_and_phase7_marker_names() {
        let mut supervisor = PythServiceSupervisor::new_for_test();
        supervisor.record_exit(ServiceKind::TaskSteward, GraphExitStatus::Ok);
        assert_eq!(
            supervisor.next_action(),
            SupervisorAction::RelaunchTaskSteward
        );
        assert!(!supervisor.service_faulted(ServiceKind::TaskSteward));

        assert_eq!(
            SESSION_MANAGER_READY_MARKER,
            "PYTHOS:PYTHTIG:SESSION_MANAGER_READY"
        );
        assert_eq!(
            TASK_STEWARD_READY_MARKER,
            "PYTHOS:PYTHTIG:TASK_STEWARD_READY"
        );
        assert_eq!(
            DEFAULT_SERVICES_READY_MARKER,
            "PYTHOS:PYTHTIG:DEFAULT_SERVICES_READY"
        );
        assert_eq!(
            SERVICE_FAULT_CONTAINED_MARKER,
            "PYTHOS:PYTHTIG:SERVICE_FAULT_CONTAINED"
        );
        assert_eq!(
            SESSION_MANAGER_FAULT_CONTAINED_MARKER,
            "PYTHOS:PYTHTIG:SERVICE_FAULT_CONTAINED service:session-manager"
        );
        assert_eq!(
            RECOVERY_SHELL_ENTER_MARKER,
            "PYTHOS:PYTHTIG:RECOVERY_SHELL_ENTER"
        );
    }
}
