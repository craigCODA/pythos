use pythc::{encode::encode_verified_graph, lower::lower_program, typecheck::typecheck_source};
use pythos_shared::{
    capability_abi::PackedCapability,
    graph_window::{Invocation, Rect, SceneResult, WindowError},
    pyth_tig::verify::verify_bytes,
};
use pythos_user_graph_window::{Scratch, evaluate};

const WORKSPACE: PackedCapability = PackedCapability::from_parts(1, 2);
const SOURCE: &str = include_str!("../window.pyth");

fn compile(source: &str) -> Vec<u8> {
    encode_verified_graph(&lower_program(&typecheck_source(source).unwrap()).unwrap()).unwrap()
}

fn run(bytes: &[u8]) -> Result<SceneResult, WindowError> {
    let invocation = Invocation::encode(bytes, WORKSPACE).unwrap();
    let invocation = Invocation::decode(&invocation).unwrap();
    evaluate(
        verify_bytes(invocation.package).unwrap(),
        invocation.workspace,
        &mut Scratch::new(),
    )
}

#[test]
fn graph_revision_controls_rendered_color_and_motion() {
    let source = SOURCE
        .replace("70F2CD", "FC8D7B")
        .replace("60|280", "60|240");
    let base = run(&compile(SOURCE)).unwrap();
    let changed = run(&compile(&source)).unwrap();
    let scene = SceneResult::decode(&changed.encode()).unwrap().scene;

    assert_eq!(
        scene.title_bar,
        Rect {
            x: 1,
            y: 1,
            width: 350,
            height: 35
        }
    );
    let animated = usize::from(scene.animation.node_index);
    assert_eq!(base.scene.nodes()[animated].color, 0x70F2CD);
    assert_eq!(scene.nodes()[animated].color, 0xFC8D7B);
    assert_eq!(base.scene.node_rect(animated, 60).unwrap().x, 280);
    assert_eq!(scene.node_rect(animated, 60).unwrap().x, 240);
    assert_eq!(scene.node_rect(animated, 45).unwrap().x, 180);
    assert_eq!(scene.node_rect(animated, 150).unwrap().x, 120);
}

#[test]
fn worker_rejects_a_track_leaving_the_window() {
    let source = SOURCE.replace("60|280", "60|340");
    assert_eq!(run(&compile(&source)), Err(WindowError::Scene));
}

#[test]
fn kernel_rejects_a_result_with_nonzero_reserved_data() {
    let mut reply = run(&compile(SOURCE)).unwrap().encode();
    *reply.last_mut().unwrap() = 1;
    assert_eq!(SceneResult::decode(&reply), Err(WindowError::Result));
}
