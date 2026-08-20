use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

use pyth_codegen_x86_64::lower::lower_verified_graph;
use pythos_shared::pyth_tig::verify::verify_bytes;

fn main() {
    if let Err(error) = run() {
        eprintln!("{}", error.message);
        process::exit(error.exit_code);
    }
}

fn run() -> Result<(), CliError> {
    let mut args = env::args_os().skip(1);
    let command = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or_else(usage_error)?;

    match command.as_str() {
        "build" => {
            let package_path = args.next().map(PathBuf::from).ok_or_else(usage_error)?;
            let flag = args
                .next()
                .and_then(|arg| arg.into_string().ok())
                .ok_or_else(usage_error)?;
            if flag != "-o" {
                return Err(usage_error());
            }
            let output_path = args.next().map(PathBuf::from).ok_or_else(usage_error)?;
            reject_extra_args(args)?;
            build(&package_path, &output_path)
        }
        "inspect" => {
            let elf_path = args.next().map(PathBuf::from).ok_or_else(usage_error)?;
            reject_extra_args(args)?;
            inspect(&elf_path)
        }
        _ => Err(usage_error()),
    }
}

fn build(package_path: &Path, output_path: &Path) -> Result<(), CliError> {
    let package = fs::read(package_path).map_err(|error| io_error(package_path, error))?;
    let graph = verify_bytes(&package).map_err(|error| verifier_error(package_path, error))?;
    let image = lower_verified_graph(graph).map_err(codegen_error)?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
    }
    fs::write(output_path, image.bytes).map_err(|error| io_error(output_path, error))?;
    println!("PYTH_NATIVE_BUILD_OK");
    Ok(())
}

fn inspect(path: &Path) -> Result<(), CliError> {
    let bytes = fs::read(path).map_err(|error| io_error(path, error))?;
    if bytes.len() < 64 || &bytes[..4] != b"\x7FELF" {
        return Err(CliError {
            exit_code: 3,
            message: format!("invalid ELF: {}", path.display()),
        });
    }
    println!("type: 0x{:04X}", read_u16(&bytes, 16)?);
    println!("machine: 0x{:04X}", read_u16(&bytes, 18)?);
    println!("entry: 0x{:016X}", read_u64(&bytes, 24)?);
    println!("program_headers: {}", read_u16(&bytes, 56)?);
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, CliError> {
    let range = bytes.get(offset..offset + 2).ok_or_else(|| CliError {
        exit_code: 3,
        message: "truncated ELF header".to_string(),
    })?;
    Ok(u16::from_le_bytes([range[0], range[1]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, CliError> {
    let range = bytes.get(offset..offset + 8).ok_or_else(|| CliError {
        exit_code: 3,
        message: "truncated ELF header".to_string(),
    })?;
    Ok(u64::from_le_bytes(range.try_into().unwrap()))
}

fn reject_extra_args(mut args: impl Iterator<Item = std::ffi::OsString>) -> Result<(), CliError> {
    if args.next().is_some() {
        Err(usage_error())
    } else {
        Ok(())
    }
}

fn verifier_error(path: &Path, error: impl core::fmt::Debug) -> CliError {
    CliError {
        exit_code: 3,
        message: format!("failed to verify {}: {error:?}", path.display()),
    }
}

fn codegen_error(error: impl core::fmt::Display) -> CliError {
    CliError {
        exit_code: 3,
        message: format!("native lowering failed: {error}"),
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
        message:
            "usage: pyth-codegen-x86_64 <build PACKAGE.TIG -o PROGRAM.ELF|inspect PROGRAM.ELF>"
                .to_string(),
    }
}

struct CliError {
    exit_code: i32,
    message: String,
}
