use pythc::{intrinsics::Intrinsic, typecheck::typecheck_source, types::PythType};

const BAD_CAP_ADD: &str = r#"
program bad_cap_add principal 0x1 {
    import log: capability<system.log, read>;
    fn main() -> unit {
        let x: u64 = log + 1;
        return;
    }
}
"#;

const BAD_REVISE_RIGHTS: &str = r#"
program bad_revise_rights principal 0x1 {
    import note: capability<object, read>;
    fn main() -> unit {
        let note_id: object_id = 1;
        let revision: revision_id = object.revise(note, note_id, 1, "hello");
        return;
    }
}
"#;

const BAD_UNKNOWN_NAME: &str = r#"
program bad_unknown_name principal 0x1 {
    import log: capability<system.log, read>;
    fn main() -> unit {
        system.log(log, missing);
        return;
    }
}
"#;

const TASK_STEWARD_INTRINSICS: &str = r#"
program task_steward principal 0x5059544853540001 {
    import context: capability<task.context, read>;
    import proposals: capability<task.proposal, create>;
    fn main() -> unit {
        let score: u64 = task.context_score(context);
        let candidate: task_id = task.context_candidate(context);
        task.propose(proposals, candidate, score);
        return;
    }
}
"#;

const TASK_PROPOSE_ASSIGNED: &str = r#"
program task_steward principal 0x5059544853540001 {
    import context: capability<task.context, read>;
    import proposals: capability<task.proposal, create>;
    fn main() -> unit {
        let score: u64 = task.context_score(context);
        let candidate: task_id = task.context_candidate(context);
        let proposal: proposal_id = task.propose(proposals, candidate, score);
        return;
    }
}
"#;

const SESSION_MANAGER_COMMAND_INTRINSICS: &str = r#"
program session_manager principal 0x50595448534D0001 {
    import commands: capability<command, read|append>;
    fn main() -> unit {
        let kind: u64 = command.kind(commands);
        let text: utf8 = command.text(commands);
        if kind == 3 {
            command.result_emit(commands, 0, text);
            return;
        } else {
            command.result_emit(commands, 0, "tasks-listed");
            return;
        }
    }
}
"#;

#[test]
fn typechecks_object_capability_flow() {
    let typed = typecheck_source(include_str!("fixtures/object-note.pyth")).unwrap();
    assert_eq!(typed.main.result_type, PythType::Unit);
    assert!(typed.required_intrinsics.contains(&Intrinsic::ObjectCreate));
    assert!(typed.required_intrinsics.contains(&Intrinsic::ObjectRevise));
}

#[test]
fn typechecks_task_context_and_proposal_intrinsics_with_proposal_only_rights() {
    let typed = typecheck_source(TASK_STEWARD_INTRINSICS).unwrap();

    assert_eq!(typed.main.result_type, PythType::Unit);
    assert!(
        typed
            .required_intrinsics
            .contains(&Intrinsic::TaskContextScore)
    );
    assert!(
        typed
            .required_intrinsics
            .contains(&Intrinsic::TaskContextCandidate)
    );
    assert!(typed.required_intrinsics.contains(&Intrinsic::TaskPropose));
}

#[test]
fn typechecks_command_read_and_result_intrinsics() {
    let typed = typecheck_source(SESSION_MANAGER_COMMAND_INTRINSICS).unwrap();

    assert_eq!(typed.main.result_type, PythType::Unit);
    assert!(typed.required_intrinsics.contains(&Intrinsic::CommandKind));
    assert!(typed.required_intrinsics.contains(&Intrinsic::CommandText));
    assert!(
        typed
            .required_intrinsics
            .contains(&Intrinsic::CommandResultEmit)
    );
}

#[test]
fn rejects_assigned_task_proposal_result_because_emit_is_effect_only() {
    assert_eq!(
        typecheck_source(TASK_PROPOSE_ASSIGNED).unwrap_err().code,
        "T0003"
    );
}

#[test]
fn rejects_capability_arithmetic_wrong_intrinsic_rights_and_unknown_name() {
    assert_eq!(typecheck_source(BAD_CAP_ADD).unwrap_err().code, "T0008");
    assert_eq!(
        typecheck_source(BAD_REVISE_RIGHTS).unwrap_err().code,
        "T0012"
    );
    assert_eq!(
        typecheck_source(BAD_UNKNOWN_NAME).unwrap_err().code,
        "T0002"
    );
}
