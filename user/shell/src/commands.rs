//! Human command-line parsing into typed object-shell requests (ADR 0051).
//!
//! `shell.elf` owns all human command grammar; PythCore never parses command
//! text, only the typed `ObjectShellRequest` this module produces.

use pythos_shared::object_shell_abi::{
    FIELD_TEXT, OBJECT_KIND_NOTE, OBJECT_SHELL_ABI_MAJOR, OBJECT_SHELL_ABI_MINOR, OP_CREATE_OBJECT,
    OP_GET_HISTORY, OP_INSPECT_OBJECT, OP_QUERY_OBJECTS, OP_REVISE_FIELD, ObjectShellRequest,
    PackedCapability,
};
use pythos_shared::task_abi::{
    OP_ABANDON_TASK, OP_APPEND_TASK_EVENT, OP_APPROVE_PROPOSAL, OP_COMPLETE_TASK, OP_CREATE_TASK,
    OP_LIST_PROPOSALS, OP_READ_ACTIVE_TASK, OP_REJECT_PROPOSAL, OP_REVIVE_TASK, OP_SUSPEND_TASK,
    TASK_ABI_MAJOR, TASK_ABI_MINOR, TASK_REQUEST_SUSPEND_CURRENT, TaskEventInput, TaskRequest,
};

const MAX_TEXT_LEN: usize = 16;
const MAX_TASK_INPUT_LEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandError {
    Empty,
    Unknown,
    BadObjectId,
    BadTaskId,
    BadProposalId,
    BadTaskEvent,
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
    Task {
        request: TaskRequest,
        input: [u8; MAX_TASK_INPUT_LEN],
        input_len: usize,
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

fn base_task_request(operation: u16) -> TaskRequest {
    TaskRequest {
        abi_major: TASK_ABI_MAJOR,
        abi_minor: TASK_ABI_MINOR,
        operation,
        proposal_kind: 0,
        authority: 0,
        task_id: 0,
        proposal_id: 0,
        target_task_id: 0,
        input_ptr: 0,
        input_len: 0,
        output_ptr: 0,
        output_len: 0,
        flags: 0,
        score: 0,
        reserved0: 0,
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
        b"task" => parse_task(rest),
        b"proposal" => parse_proposal(rest),
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

fn parse_task(rest: &[u8]) -> Result<Command, CommandError> {
    let (subcommand, rest) = split_once_space(rest);
    let rest = trim(rest);
    match subcommand {
        b"new" => parse_task_new(rest),
        b"active" if rest.is_empty() => task_command(base_task_request(OP_READ_ACTIVE_TASK), &[]),
        b"list" if rest.is_empty() => task_command(base_task_request(OP_READ_ACTIVE_TASK), &[]),
        b"event" => parse_task_event(rest),
        b"suspend" => parse_task_transition(rest, OP_SUSPEND_TASK),
        b"revive" => parse_task_transition(rest, OP_REVIVE_TASK),
        b"complete" => parse_task_transition(rest, OP_COMPLETE_TASK),
        b"abandon" => parse_task_transition(rest, OP_ABANDON_TASK),
        _ => Err(CommandError::Unknown),
    }
}

fn parse_proposal(rest: &[u8]) -> Result<Command, CommandError> {
    let (subcommand, rest) = split_once_space(rest);
    let rest = trim(rest);
    match subcommand {
        b"list" if rest.is_empty() => task_command(base_task_request(OP_LIST_PROPOSALS), &[]),
        b"approve" => parse_proposal_approval(rest),
        b"reject" => parse_proposal_reject(rest),
        _ => Err(CommandError::Unknown),
    }
}

fn parse_task_new(rest: &[u8]) -> Result<Command, CommandError> {
    let title = parse_bare_quoted_text(rest)?;
    if title.len() > MAX_TASK_INPUT_LEN {
        return Err(CommandError::TextTooLong);
    }
    task_command(base_task_request(OP_CREATE_TASK), title)
}

fn parse_task_event(rest: &[u8]) -> Result<Command, CommandError> {
    let (tag_field, rest) = split_once_space(rest);
    let (tool_field, rest) = split_once_space(trim(rest));
    let (object_kind_field, trailing) = split_once_space(trim(rest));
    if !trim(trailing).is_empty() {
        return Err(CommandError::Unknown);
    }
    let input = TaskEventInput {
        tag_hash: parse_hex_field(tag_field, b"tag:")?,
        tool_domain: parse_u16_field(tool_field, b"tool:")?,
        object_kind: parse_u16_field(object_kind_field, b"object-kind:")?,
        flags: 0,
        reserved0: 0,
    };
    let mut bytes = [0u8; 16];
    bytes[0..8].copy_from_slice(&input.tag_hash.to_le_bytes());
    bytes[8..10].copy_from_slice(&input.object_kind.to_le_bytes());
    bytes[10..12].copy_from_slice(&input.tool_domain.to_le_bytes());
    bytes[12..14].copy_from_slice(&input.flags.to_le_bytes());
    bytes[14..16].copy_from_slice(&input.reserved0.to_le_bytes());
    task_command(base_task_request(OP_APPEND_TASK_EVENT), &bytes)
}

fn parse_task_transition(rest: &[u8], operation: u16) -> Result<Command, CommandError> {
    let task_id = parse_decimal_u64(rest).map_err(|_| CommandError::BadTaskId)?;
    let mut request = base_task_request(operation);
    request.task_id = task_id;
    task_command(request, &[])
}

fn parse_proposal_approval(rest: &[u8]) -> Result<Command, CommandError> {
    let (proposal_id, mode) = split_once_space(rest);
    let proposal_id = parse_decimal_u64(proposal_id).map_err(|_| CommandError::BadProposalId)?;
    let mode = trim(mode);
    let mut request = base_task_request(OP_APPROVE_PROPOSAL);
    request.proposal_id = proposal_id;
    match mode {
        b"keep-current" => {}
        b"suspend-current" => request.flags = TASK_REQUEST_SUSPEND_CURRENT,
        _ => return Err(CommandError::Unknown),
    }
    task_command(request, &[])
}

fn parse_proposal_reject(rest: &[u8]) -> Result<Command, CommandError> {
    let proposal_id = parse_decimal_u64(rest).map_err(|_| CommandError::BadProposalId)?;
    let mut request = base_task_request(OP_REJECT_PROPOSAL);
    request.proposal_id = proposal_id;
    task_command(request, &[])
}

fn task_command(request: TaskRequest, input_bytes: &[u8]) -> Result<Command, CommandError> {
    if input_bytes.len() > MAX_TASK_INPUT_LEN {
        return Err(CommandError::TextTooLong);
    }
    let mut input = [0u8; MAX_TASK_INPUT_LEN];
    input[..input_bytes.len()].copy_from_slice(input_bytes);
    Ok(Command::Task {
        request,
        input,
        input_len: input_bytes.len(),
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

fn parse_bare_quoted_text(field: &[u8]) -> Result<&[u8], CommandError> {
    let after_quote = field.strip_prefix(b"\"").ok_or(CommandError::Unknown)?;
    let close = after_quote
        .iter()
        .position(|&byte| byte == b'"')
        .ok_or(CommandError::Unknown)?;
    if !trim(&after_quote[close + 1..]).is_empty() {
        return Err(CommandError::Unknown);
    }
    Ok(&after_quote[..close])
}

fn parse_hex_field(field: &[u8], prefix: &[u8]) -> Result<u64, CommandError> {
    let digits = field
        .strip_prefix(prefix)
        .ok_or(CommandError::BadTaskEvent)?;
    let digits = digits.strip_prefix(b"0x").unwrap_or(digits);
    if digits.is_empty() {
        return Err(CommandError::BadTaskEvent);
    }
    let mut value = 0u64;
    for &byte in digits {
        let nibble = match byte {
            b'0'..=b'9' => u64::from(byte - b'0'),
            b'a'..=b'f' => u64::from(byte - b'a' + 10),
            b'A'..=b'F' => u64::from(byte - b'A' + 10),
            _ => return Err(CommandError::BadTaskEvent),
        };
        value = value
            .checked_mul(16)
            .and_then(|v| v.checked_add(nibble))
            .ok_or(CommandError::BadTaskEvent)?;
    }
    Ok(value)
}

fn parse_u16_field(field: &[u8], prefix: &[u8]) -> Result<u16, CommandError> {
    let digits = field
        .strip_prefix(prefix)
        .ok_or(CommandError::BadTaskEvent)?;
    let value = parse_decimal_u64(digits).map_err(|_| CommandError::BadTaskEvent)?;
    u16::try_from(value).map_err(|_| CommandError::BadTaskEvent)
}

fn parse_decimal_u64(digits: &[u8]) -> Result<u64, ()> {
    if digits.is_empty() {
        return Err(());
    }
    let mut value = 0u64;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return Err(());
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(byte - b'0')))
            .ok_or(())?;
    }
    Ok(value)
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
    fn parses_task_and_proposal_commands() {
        match parse_command(br#"task new "Universal Boot""#).unwrap() {
            Command::Task {
                request,
                input,
                input_len,
            } => {
                assert_eq!(request.operation, OP_CREATE_TASK);
                assert_eq!(&input[..input_len], b"Universal Boot");
            }
            _ => panic!("expected task command"),
        }
        assert!(matches!(
            parse_command(b"task active").unwrap(),
            Command::Task { request, .. } if request.operation == OP_READ_ACTIVE_TASK
        ));
        assert!(matches!(
            parse_command(b"task list").unwrap(),
            Command::Task { request, .. } if request.operation == OP_READ_ACTIVE_TASK
        ));
        match parse_command(b"task event tag:50595448 tool:2 object-kind:20").unwrap() {
            Command::Task {
                request,
                input,
                input_len,
            } => {
                assert_eq!(request.operation, OP_APPEND_TASK_EVENT);
                assert_eq!(input_len, core::mem::size_of::<TaskEventInput>());
                assert_eq!(
                    u64::from_le_bytes(input[0..8].try_into().unwrap()),
                    0x5059_5448
                );
                assert_eq!(u16::from_le_bytes(input[8..10].try_into().unwrap()), 20);
                assert_eq!(u16::from_le_bytes(input[10..12].try_into().unwrap()), 2);
            }
            _ => panic!("expected task event command"),
        }
        assert!(matches!(
            parse_command(b"task suspend 3001").unwrap(),
            Command::Task { request, .. } if request.operation == OP_SUSPEND_TASK && request.task_id == 3001
        ));
        assert!(matches!(
            parse_command(b"task revive 3001").unwrap(),
            Command::Task { request, .. } if request.operation == OP_REVIVE_TASK && request.task_id == 3001
        ));
        assert!(matches!(
            parse_command(b"task complete 3001").unwrap(),
            Command::Task { request, .. } if request.operation == OP_COMPLETE_TASK && request.task_id == 3001
        ));
        assert!(matches!(
            parse_command(b"task abandon 3001").unwrap(),
            Command::Task { request, .. } if request.operation == OP_ABANDON_TASK && request.task_id == 3001
        ));
        assert!(matches!(
            parse_command(b"proposal list").unwrap(),
            Command::Task { request, .. } if request.operation == OP_LIST_PROPOSALS
        ));
        assert!(matches!(
            parse_command(b"proposal approve 4001 suspend-current").unwrap(),
            Command::Task { request, .. } if request.operation == OP_APPROVE_PROPOSAL
                && request.proposal_id == 4001
                && request.flags == TASK_REQUEST_SUSPEND_CURRENT
        ));
        assert!(matches!(
            parse_command(b"proposal approve 4001 keep-current").unwrap(),
            Command::Task { request, .. } if request.operation == OP_APPROVE_PROPOSAL
                && request.proposal_id == 4001
                && request.flags == 0
        ));
        assert!(matches!(
            parse_command(b"proposal reject 4001").unwrap(),
            Command::Task { request, .. } if request.operation == OP_REJECT_PROPOSAL
                && request.proposal_id == 4001
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
