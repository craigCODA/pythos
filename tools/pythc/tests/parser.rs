use pythc::parser::parse_source;

#[test]
fn parses_minimal_program_and_capability_import() {
    let source = r#"
program hello principal 0x5059544847520001 {
    import log: capability<system.log, write>;
    fn main() -> unit {
        system.log(log, "hello");
        return;
    }
}
"#;
    let program = parse_source(source).unwrap();
    assert_eq!(program.name.text, "hello");
    assert_eq!(program.principal_id, 0x5059_5448_4752_0001);
    assert_eq!(program.imports.len(), 1);
    assert_eq!(program.main.statements.len(), 2);
}

#[test]
fn rejects_unbudgeted_while_and_second_function() {
    let unbudgeted = "program x principal 0x1 { fn main() -> unit { while true { return; } } }";
    assert_eq!(parse_source(unbudgeted).unwrap_err().code, "P0007");

    let second =
        "program x principal 0x1 { fn main() -> unit { return; } fn other() -> unit { return; } }";
    assert_eq!(parse_source(second).unwrap_err().code, "P0011");
}
