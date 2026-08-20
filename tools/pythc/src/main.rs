use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use pythc::{
    diagnostic::{Diagnostic, render_diagnostic},
    encode::encode_verified_graph,
    lower::lower_program,
    typecheck::typecheck_source,
};
use pythos_shared::pyth_tig::format::PythGraphPackage;

fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{}", error.message);
            process::exit(error.exit_code);
        }
    }
}

fn run() -> Result<(), CliError> {
    let mut args = env::args_os().skip(1);
    let command = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or_else(usage_error)?;

    match command.as_str() {
        "check" => {
            let source_path = args.next().map(PathBuf::from).ok_or_else(usage_error)?;
            reject_extra_args(args)?;
            let source = read_source(&source_path)?;
            typecheck_source(&source)
                .map_err(|diagnostic| source_error(&source_path, &source, diagnostic))?;
            println!("PYTHC_CHECK_OK");
            Ok(())
        }
        "build" => {
            let source_path = args.next().map(PathBuf::from).ok_or_else(usage_error)?;
            let flag = args
                .next()
                .and_then(|arg| arg.into_string().ok())
                .ok_or_else(usage_error)?;
            if flag != "-o" {
                return Err(usage_error());
            }
            let output_path = args.next().map(PathBuf::from).ok_or_else(usage_error)?;
            reject_extra_args(args)?;
            let source = read_source(&source_path)?;
            let typed = typecheck_source(&source)
                .map_err(|diagnostic| source_error(&source_path, &source, diagnostic))?;
            let graph = lower_program(&typed).map_err(package_error)?;
            let bytes = encode_verified_graph(&graph).map_err(package_error)?;
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
            }
            fs::write(&output_path, bytes).map_err(|error| io_error(&output_path, error))?;
            println!("PYTHC_BUILD_OK");
            Ok(())
        }
        "inspect" => {
            let package_path = args.next().map(PathBuf::from).ok_or_else(usage_error)?;
            reject_extra_args(args)?;
            let bytes = fs::read(&package_path).map_err(|error| io_error(&package_path, error))?;
            let package = PythGraphPackage::decode(&bytes).map_err(|error| CliError {
                exit_code: 3,
                message: format!("failed to decode {}: {error:?}", package_path.display()),
            })?;
            print_inspection(&package);
            Ok(())
        }
        _ => Err(usage_error()),
    }
}

fn read_source(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|error| io_error(path, error))
}

fn reject_extra_args(mut args: impl Iterator<Item = std::ffi::OsString>) -> Result<(), CliError> {
    if args.next().is_some() {
        Err(usage_error())
    } else {
        Ok(())
    }
}

fn source_error(path: &Path, source: &str, diagnostic: Diagnostic) -> CliError {
    CliError {
        exit_code: 2,
        message: render_diagnostic(path, source, &diagnostic),
    }
}

fn package_error(diagnostic: Diagnostic) -> CliError {
    CliError {
        exit_code: 3,
        message: format!("error[{}]: {}", diagnostic.code, diagnostic.message),
    }
}

fn io_error(path: &Path, error: std::io::Error) -> CliError {
    CliError {
        exit_code: 4,
        message: format!("I/O error for {}: {error}", path.display()),
    }
}

fn usage_error() -> CliError {
    CliError {
        exit_code: 4,
        message: "usage: pythc <check SOURCE|build SOURCE -o PACKAGE|inspect PACKAGE>".to_string(),
    }
}

fn print_inspection(package: &PythGraphPackage<'_>) {
    let header = package.header();
    let principal = header.principal_id;
    let checksum = header.checksum;
    println!("program: {}", program_name(package.string_table()));
    println!("principal: 0x{principal:016X}");
    println!("imports: {}", package.imports().len());
    println!("blocks: {}", package.blocks().len());
    println!("nodes: {}", package.nodes().len());
    println!("checksum: 0x{checksum:016X}");
}

fn program_name(string_table: &[u8]) -> String {
    let end = string_table
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(string_table.len());
    String::from_utf8_lossy(&string_table[..end]).into_owned()
}

struct CliError {
    exit_code: i32,
    message: String,
}
