pub const MAX_LINE_LEN: usize = 96;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineAction {
    None,
    Echo(u8),
    Erase,
    Execute {
        line: [u8; MAX_LINE_LEN],
        len: usize,
    },
    RejectOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineEditor {
    line: [u8; MAX_LINE_LEN],
    len: usize,
    overflowed: bool,
    suppress_next_lf: bool,
}

impl LineEditor {
    pub const fn new() -> Self {
        Self {
            line: [0; MAX_LINE_LEN],
            len: 0,
            overflowed: false,
            suppress_next_lf: false,
        }
    }

    pub fn input(&mut self, byte: u8) -> LineAction {
        if self.suppress_next_lf {
            self.suppress_next_lf = false;
            if byte == b'\n' {
                return LineAction::None;
            }
        }

        if byte == b'\r' || byte == b'\n' {
            let rejected = self.overflowed;
            let line = self.line;
            let len = self.len;
            self.reset();
            self.suppress_next_lf = byte == b'\r';
            return if rejected {
                LineAction::RejectOverflow
            } else {
                LineAction::Execute { line, len }
            };
        }

        if byte == 0x08 || byte == 0x7F {
            if self.overflowed || self.len == 0 {
                return LineAction::None;
            }
            self.len -= 1;
            self.line[self.len] = 0;
            return LineAction::Erase;
        }

        if self.overflowed {
            return LineAction::None;
        }
        if self.len == MAX_LINE_LEN {
            self.overflowed = true;
            return LineAction::None;
        }
        self.line[self.len] = byte;
        self.len += 1;
        LineAction::Echo(byte)
    }

    fn reset(&mut self) {
        self.line = [0; MAX_LINE_LEN];
        self.len = 0;
        self.overflowed = false;
    }
}

impl Default for LineEditor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{LineAction, LineEditor, MAX_LINE_LEN};

    #[test]
    fn overflowing_line_is_rejected_instead_of_executing_truncated_prefix() {
        let mut editor = LineEditor::new();
        for &byte in b"inspect object:1042" {
            assert!(matches!(editor.input(byte), LineAction::Echo(_)));
        }
        for _ in 0..MAX_LINE_LEN {
            assert!(!matches!(
                editor.input(b' '),
                LineAction::Execute { .. } | LineAction::RejectOverflow
            ));
        }

        assert_eq!(editor.input(b'\n'), LineAction::RejectOverflow);
    }

    #[test]
    fn crlf_executes_one_line_not_an_empty_second_line() {
        let mut editor = LineEditor::new();
        for &byte in b"help" {
            assert!(matches!(editor.input(byte), LineAction::Echo(_)));
        }

        assert!(matches!(
            editor.input(b'\r'),
            LineAction::Execute { len: 4, .. }
        ));
        assert_eq!(editor.input(b'\n'), LineAction::None);
    }

    #[test]
    fn backspace_deletes_previous_byte_before_execute() {
        let mut editor = LineEditor::new();
        for &byte in b"helq" {
            assert!(matches!(editor.input(byte), LineAction::Echo(_)));
        }

        assert_eq!(editor.input(0x08), LineAction::Erase);
        assert_eq!(editor.input(b'p'), LineAction::Echo(b'p'));
        let LineAction::Execute { line, len } = editor.input(b'\r') else {
            panic!("expected edited line to execute");
        };

        assert_eq!(&line[..len], b"help");
    }

    #[test]
    fn delete_key_deletes_previous_byte_before_execute() {
        let mut editor = LineEditor::new();
        for &byte in b"helo" {
            assert!(matches!(editor.input(byte), LineAction::Echo(_)));
        }

        assert_eq!(editor.input(0x7F), LineAction::Erase);
        assert_eq!(editor.input(b'p'), LineAction::Echo(b'p'));
        let LineAction::Execute { line, len } = editor.input(b'\n') else {
            panic!("expected edited line to execute");
        };

        assert_eq!(&line[..len], b"help");
    }
}
