use crate::pyth_runtime_abi::GRAPH_EXIT_OK;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionGraphLifecycleAction {
    Reinvoke,
    RequestRecovery,
}

pub const fn session_graph_lifecycle_action(status: u16) -> SessionGraphLifecycleAction {
    if status == GRAPH_EXIT_OK {
        SessionGraphLifecycleAction::Reinvoke
    } else {
        SessionGraphLifecycleAction::RequestRecovery
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pyth_runtime_abi::{GRAPH_EXIT_BUDGET_EXHAUSTED, GRAPH_EXIT_RUNTIME_ERROR};

    #[test]
    fn successful_graph_exit_reinvokes_the_session_graph() {
        assert_eq!(
            session_graph_lifecycle_action(GRAPH_EXIT_OK),
            SessionGraphLifecycleAction::Reinvoke
        );
    }

    #[test]
    fn failed_or_unknown_graph_exit_requests_recovery() {
        for status in [
            GRAPH_EXIT_RUNTIME_ERROR,
            GRAPH_EXIT_BUDGET_EXHAUSTED,
            u16::MAX,
        ] {
            assert_eq!(
                session_graph_lifecycle_action(status),
                SessionGraphLifecycleAction::RequestRecovery
            );
        }
    }
}
