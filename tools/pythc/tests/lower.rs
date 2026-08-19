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
