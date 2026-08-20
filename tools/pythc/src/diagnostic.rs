use std::path::Path;

pub use crate::span::Diagnostic;

pub fn render_diagnostic(path: &Path, source: &str, diagnostic: &Diagnostic) -> String {
    let (line_number, column_number, line_start, line_end) =
        line_info(source, diagnostic.span.start);
    let line = &source[line_start..line_end];
    let caret_len = diagnostic
        .span
        .end
        .saturating_sub(diagnostic.span.start)
        .max(1);
    let caret_prefix = " ".repeat(column_number.saturating_sub(1));
    let carets = "^".repeat(caret_len.min(line.len().saturating_sub(column_number - 1).max(1)));

    format!(
        "error[{}]: {}\n --> {}:{}:{}\n  |\n{} | {}\n  | {}{}",
        diagnostic.code,
        diagnostic.message,
        path.display(),
        line_number,
        column_number,
        line_number,
        line,
        caret_prefix,
        carets
    )
}

fn line_info(source: &str, offset: usize) -> (usize, usize, usize, usize) {
    let offset = offset.min(source.len());
    let mut line_number = 1usize;
    let mut line_start = 0usize;
    for (index, byte) in source.bytes().enumerate().take(offset) {
        if byte == b'\n' {
            line_number += 1;
            line_start = index + 1;
        }
    }
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |delta| line_start + delta);
    let column_number = offset.saturating_sub(line_start) + 1;
    (line_number, column_number, line_start, line_end)
}
