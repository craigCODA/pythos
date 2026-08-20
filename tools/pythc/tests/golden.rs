use std::{fs, process::Command};

#[test]
fn hello_build_is_reproducible_through_cli() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let target = root.join("target").join("pythc-golden");
    fs::create_dir_all(&target).unwrap();
    let source = root
        .join("tools")
        .join("pythc")
        .join("tests")
        .join("fixtures")
        .join("hello.pyth");
    let out1 = target.join("hello-1.pytig");
    let out2 = target.join("hello-2.pytig");

    run_build(&root, &source, &out1);
    run_build(&root, &source, &out2);

    assert_eq!(fs::read(out1).unwrap(), fs::read(out2).unwrap());
}

fn run_build(root: &std::path::Path, source: &std::path::Path, output: &std::path::Path) {
    let status = Command::new("cargo")
        .current_dir(root)
        .args(["run", "-q", "-p", "pythc", "--", "build"])
        .arg(source)
        .args(["-o"])
        .arg(output)
        .status()
        .unwrap();
    assert!(status.success());
}
