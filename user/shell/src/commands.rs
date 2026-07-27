//! Human command-line parsing into typed object-shell requests (ADR 0051).
//!
//! `shell.elf` owns all human command grammar; PythCore never parses command
//! text, only the typed `ObjectShellRequest` this module produces.

use pythos_shared::object_shell_abi::{
    FIELD_TEXT, OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MAJOR, OBJECT_SHELL_ABI_MINOR, OP_CREATE_OBJECT,
    OP_GET_HISTORY, OP_INSPECT_OBJECT, OP_QUERY_OBJECTS, OP_REVISE_FIELD, ObjectShellRequest,
    PackedCapability,
};

const MAX_TEXT_LEN: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandError {
    Empty,
    Unknown,
    BadObjectId,
    TextTooLong,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Command {
    Help,
    Reboot,
    Object {
        request: ObjectShellRequest,
        text: [u8; MAX_TEXT_LEN],
        text_len: usize,
    },
}

/// A zeroed request template with only the ABI version and operation set.
/// `authority` starts at a null handle; the caller (the shell's dispatch
/// layer, not this parser) fills it from the capability map before the
/// syscall, per operation.
fn base_request(operation: u16) -> ObjectShellRequest {
    ObjectShellRequest {
        abi_major: OBJECT_SHELL_ABI_MAJOR,
        abi_minor: OBJECT_SHELL_ABI_MINOR,
        operation,
        object_kind: 0,
        field_id: 0,
        reserved0: 0,
        authority: PackedCapability::from_raw(0),
        object_id: 0,
        input_ptr: 0,
        input_len: 0,
        output_ptr: 0,
        output_len: 0,
        reserved1: 0,
        reserved2: 0,
    }
}

/// Parse one command line into a typed `Command`. Never touches a syscall.
pub fn parse_command(line: &[u8]) -> Result<Command, CommandError> {
    let line = trim(line);
    if line.is_empty() {
        return Err(CommandError::Empty);
    }
    let (verb, rest) = split_once_space(line);
    let rest = trim(rest);
    match verb {
        b"help" if rest.is_empty() => Ok(Command::Help),
        b"reboot" if rest.is_empty() => Ok(Command::Reboot),
        b"query" => parse_kind_only(rest, OP_QUERY_OBJECTS),
        b"create" => parse_kind_only(rest, OP_CREATE_OBJECT),
        b"inspect" => parse_object_only(rest, OP_INSPECT_OBJECT),
        b"history" => parse_object_only(rest, OP_GET_HISTORY),
        b"revise" => parse_revise(rest),
        _ => Err(CommandError::Unknown),
    }
}

fn parse_kind_only(rest: &[u8], operation: u16) -> Result<Command, CommandError> {
    if rest != b"kind:note" {
        return Err(CommandError::Unknown);
    }
    let mut request = base_request(operation);
    request.object_kind = OBJECT_KIND_NOTE;
    Ok(Command::Object {
        request,
        text: [0; MAX_TEXT_LEN],
        text_len: 0,
    })
}

fn parse_object_only(rest: &[u8], operation: u16) -> Result<Command, CommandError> {
    let object_id = parse_object_id_field(rest)?;
    let mut request = base_request(operation);
    request.object_id = object_id;
    Ok(Command::Object {
        request,
        text: [0; MAX_TEXT_LEN],
        text_len: 0,
    })
}

fn parse_revise(rest: &[u8]) -> Result<Command, CommandError> {
    let (object_part, remainder) = split_once_space(rest);
    let object_id = parse_object_id(object_part)?;
    let remainder = trim(remainder);
    let text_bytes = parse_quoted_text_field(remainder, b"text=")?;
    if text_bytes.len() > MAX_TEXT_LEN {
        return Err(CommandError::TextTooLong);
    }
    let mut text = [0u8; MAX_TEXT_LEN];
    text[..text_bytes.len()].copy_from_slice(text_bytes);

    let mut request = base_request(OP_REVISE_FIELD);
    request.object_id = object_id;
    request.field_id = FIELD_TEXT;
    Ok(Command::Object {
        request,
        text,
        text_len: text_bytes.len(),
    })
}

/// Parse `object:<decimal>` as the entire field.
fn parse_object_id_field(field: &[u8]) -> Result<u64, CommandError> {
    parse_object_id(field)
}

fn parse_object_id(field: &[u8]) -> Result<u64, CommandError> {
    let digits = field
        .strip_prefix(b"object:")
        .ok_or(CommandError::BadObjectId)?;
    if digits.is_empty() {
        return Err(CommandError::BadObjectId);
    }
    let mut value: u64 = 0;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return Err(CommandError::BadObjectId);
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(byte - b'0')))
            .ok_or(CommandError::BadObjectId)?;
    }
    Ok(value)
}

/// Parse `<prefix>"<text>"` (e.g. `text="hello"`), returning the bytes
/// between the quotes.
fn parse_quoted_text_field<'a>(field: &'a [u8], prefix: &[u8]) -> Result<&'a [u8], CommandError> {
    let after_prefix = field.strip_prefix(prefix).ok_or(CommandError::Unknown)?;
    let after_quote = after_prefix
        .strip_prefix(b"\"")
        .ok_or(CommandError::Unknown)?;
    let close = after_quote
        .iter()
        .position(|&byte| byte == b'"')
        .ok_or(CommandError::Unknown)?;
    if !trim(&after_quote[close + 1..]).is_empty() {
        return Err(CommandError::Unknown);
    }
    Ok(&after_quote[..close])
}

fn trim(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|&byte| byte != b' ')
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&byte| byte != b' ')
        .map_or(start, |p| p + 1);
    &bytes[start..end]
}

fn split_once_space(line: &[u8]) -> (&[u8], &[u8]) {
    match line.iter().position(|&byte| byte == b' ') {
        Some(index) => (&line[..index], &line[index + 1..]),
        None => (line, b""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_human_commands_into_typed_requests() {
        assert_eq!(parse_command(b"help").unwrap(), Command::Help);
        assert_eq!(parse_command(b"reboot").unwrap(), Command::Reboot);
        assert!(matches!(
            parse_command(b"query kind:note").unwrap(),
            Command::Object { request, .. } if request.operation == OP_QUERY_OBJECTS
        ));
        assert!(matches!(
            parse_command(b"create kind:note").unwrap(),
            Command::Object { request, .. } if request.operation == OP_CREATE_OBJECT
        ));
        assert!(matches!(
            parse_command(b"inspect object:1042").unwrap(),
            Command::Object { request, .. } if request.operation == OP_INSPECT_OBJECT && request.object_id == 1042
        ));
        let revised = parse_command(br#"revise object:1042 text="hello""#).unwrap();
        match revised {
            Command::Object {
                request,
                text,
                text_len,
            } => {
                assert_eq!(request.operation, OP_REVISE_FIELD);
                assert_eq!(request.field_id, FIELD_TEXT);
                assert_eq!(&text[..text_len], b"hello");
            }
            _ => panic!("expected object command"),
        }
        assert!(matches!(
            parse_command(b"history object:1042").unwrap(),
            Command::Object { request, .. } if request.operation == OP_GET_HISTORY
        ));
    }

    #[test]
    fn rejects_shell_grammar_errors_before_syscall() {
        assert_eq!(parse_command(b""), Err(CommandError::Empty));
        assert_eq!(parse_command(b"ls /"), Err(CommandError::Unknown));
        assert_eq!(
            parse_command(b"inspect object:notanumber"),
            Err(CommandError::BadObjectId)
        );
    }

    #[test]
    fn revise_text_longer_than_buffer_is_rejected() {
        assert_eq!(
            parse_command(br#"revise object:1042 text="this text is far too long""#),
            Err(CommandError::TextTooLong)
        );
    }

    #[test]
    fn revise_rejects_characters_after_closing_quote() {
        assert_eq!(
            parse_command(br#"revise object:1042 text="hello" trailing"#),
            Err(CommandError::Unknown)
        );
    }

    #[test]
    fn unsupported_object_kind_is_rejected() {
        assert_eq!(
            parse_command(b"query kind:unknown"),
            Err(CommandError::Unknown)
        );
    }
}
