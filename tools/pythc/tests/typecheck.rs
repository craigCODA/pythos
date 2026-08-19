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

#[test]
fn typechecks_object_capability_flow() {
    let typed = typecheck_source(include_str!("fixtures/object-note.pyth")).unwrap();
    assert_eq!(typed.main.result_type, PythType::Unit);
    assert!(typed.required_intrinsics.contains(&Intrinsic::ObjectCreate));
    assert!(typed.required_intrinsics.contains(&Intrinsic::ObjectRevise));
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
