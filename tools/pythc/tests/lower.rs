use pythc::{encode::encode_verified_graph, lower::lower_program, typecheck::typecheck_source};
use pythos_shared::pyth_tig::{format::PythGraphPackage, opcode::Opcode, verify::verify_package};

#[test]
fn lowering_builds_single_effect_chain_and_block_parameters() {
    let typed = typecheck_source(include_str!("fixtures/branch-log.pyth")).unwrap();
    let graph = lower_program(&typed).unwrap();
    let bytes = encode_verified_graph(&graph).unwrap();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    let verified = verify_package(&package).unwrap();

    assert_eq!(verified.package().blocks().len(), 4);
    assert_eq!(graph.effect_forks(), 0);
    assert!(graph.contains_opcode(Opcode::Branch));
    assert!(graph.contains_opcode(Opcode::SystemLog));
}

#[test]
fn lowering_emits_explicit_budgeted_loop_back_edge() {
    let typed = typecheck_source(include_str!("fixtures/budget-loop.pyth")).unwrap();
    let graph = lower_program(&typed).unwrap();
    assert!(graph.has_back_edge());
    assert_eq!(graph.loop_budget_literals(), vec![8]);
}

#[test]
fn lowering_emits_object_create_flow_shape_accepted_by_verifier() {
    let typed = typecheck_source(include_str!(
        "../../../programs/examples/object-create.pyth"
    ))
    .unwrap();
    let graph = lower_program(&typed).unwrap();
    let bytes = encode_verified_graph(&graph).unwrap();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    let verified = verify_package(&package).unwrap();

    assert_eq!(verified.package().nodes().len(), 11);
    assert_eq!(verified.package().blocks().len(), 1);
    assert!(graph.contains_opcode(Opcode::ObjectCreate));
    assert!(graph.contains_opcode(Opcode::ObjectRevise));
    assert!(graph.contains_opcode(Opcode::ObjectInspect));
}

#[test]
fn object_create_package_defined_lowers_to_graph_kind_token() {
    let typed = typecheck_source(
        r#"
program package_defined_object_create principal 0x5059544847520007 {
    import workspace: capability<object.workspace, create>;
    fn main() -> unit {
        let object_id: object_id = object.create(workspace, 2);
        return;
    }
}
"#,
    )
    .unwrap();
    let graph = lower_program(&typed).unwrap();

    assert!(
        graph
            .string_table
            .windows(b"package-defined".len())
            .any(|window| window == b"package-defined")
    );
}

#[test]
fn lowering_emits_object_restore_flow_shape_accepted_by_verifier() {
    let typed = typecheck_source(include_str!(
        "../../../programs/examples/object-restore.pyth"
    ))
    .unwrap();
    let graph = lower_program(&typed).unwrap();
    let bytes = encode_verified_graph(&graph).unwrap();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    let verified = verify_package(&package).unwrap();

    assert_eq!(verified.package().nodes().len(), 10);
    assert_eq!(verified.package().blocks().len(), 1);
    assert!(graph.contains_opcode(Opcode::ObjectQuery));
    assert!(graph.contains_opcode(Opcode::ObjectInspect));
    assert!(graph.contains_opcode(Opcode::ObjectHistory));
}

#[test]
fn lowering_emits_task_context_and_proposal_shape_accepted_by_verifier() {
    let typed = typecheck_source(
        r#"
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
"#,
    )
    .unwrap();
    let graph = lower_program(&typed).unwrap();
    let bytes = encode_verified_graph(&graph).unwrap();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    let verified = verify_package(&package).unwrap();

    assert_eq!(verified.package().blocks().len(), 1);
    assert!(graph.contains_opcode(Opcode::TaskContextRead));
    assert!(graph.contains_opcode(Opcode::TaskProposalEmit));
}

#[test]
fn lowering_keeps_task_context_reads_bound_to_each_context_capability() {
    let typed = typecheck_source(
        r#"
program task_steward principal 0x5059544853540001 {
    import active_context: capability<task.context, read>;
    import candidate_context: capability<task.context, read>;
    import proposals: capability<task.proposal, create>;
    fn main() -> unit {
        let score: u64 = task.context_score(active_context);
        let candidate: task_id = task.context_candidate(candidate_context);
        task.propose(proposals, candidate, score);
        return;
    }
}
"#,
    )
    .unwrap();
    let graph = lower_program(&typed).unwrap();
    let task_context_reads = graph
        .nodes
        .iter()
        .filter(|node| node.opcode == Opcode::TaskContextRead)
        .count();

    assert_eq!(task_context_reads, 2);
}

#[test]
fn lowering_accepts_task_steward_program_with_conditional_proposal() {
    let typed = typecheck_source(include_str!("../../../programs/task-steward/main.pyth")).unwrap();
    let graph = lower_program(&typed).unwrap();

    assert_eq!(graph.effect_forks(), 0);
    let bytes = encode_verified_graph(&graph).unwrap();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    let verified = verify_package(&package).unwrap();

    assert_eq!(verified.package().blocks().len(), 3);
    assert!(graph.contains_opcode(Opcode::TaskContextRead));
    assert!(graph.contains_opcode(Opcode::TaskProposalEmit));
    assert!(graph.contains_opcode(Opcode::SystemLog));
}

#[test]
fn lowering_emits_session_manager_command_shape_accepted_by_verifier() {
    let typed =
        typecheck_source(include_str!("../../../programs/session-manager/main.pyth")).unwrap();
    let graph = lower_program(&typed).unwrap();
    let bytes = encode_verified_graph(&graph).unwrap();
    let package = PythGraphPackage::decode(&bytes).unwrap();
    let verified = verify_package(&package).unwrap();

    assert!(verified.package().blocks().len() >= 3);
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|node| node.opcode == Opcode::CommandRead)
            .count(),
        1
    );
    assert!(graph.contains_opcode(Opcode::CommandResultEmit));
}
