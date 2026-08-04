mod encode;
mod mutate;

use std::{env, fs, path::PathBuf, process};

use pythos_shared::pyth_tig::verify::verify_bytes;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let command = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or_else(usage)?;
    let path = args.next().map(PathBuf::from).ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage());
    }

    match command.as_str() {
        "emit-minimal-log" => {
            let bytes = encode::minimal_log_package();
            write_package(&path, bytes)
        }
        "emit-budget-loop" => {
            let bytes = encode::budget_loop_package();
            write_package(&path, bytes)
        }
        "emit-invalid-effect-fork" => {
            let bytes = encode::invalid_effect_fork_package();
            write_package(&path, bytes)
        }
        "verify" => {
            let bytes = fs::read(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            verify_bytes(&bytes).map_err(|error| format!("PYTH_TIG_VERIFY_ERR {error:?}"))?;
            println!("PYTH_TIG_VERIFY_OK");
            Ok(())
        }
        "mutate-suite" => {
            let bytes = fs::read(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            mutate::run_mutation_suite(&bytes)
        }
        _ => Err(usage()),
    }
}

fn write_package(path: &PathBuf, bytes: Vec<u8>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(path, bytes).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn usage() -> String {
    "usage: pyth-tig-tool <emit-minimal-log|emit-budget-loop|emit-invalid-effect-fork|verify|mutate-suite> <path>".to_string()
}
