use crate::architecture::x86_64::timer;
use crate::framebuffer::{self, TerminalTextRole};
use core::hint;
use pythos_shared::boot_protocol::PythFramebufferInfo;
use pythos_shared::evidence_log::EvidenceLogSnapshot;

const GLYPH_W: u64 = 8;
const GLYPH_H: u64 = 8;
const SCALE: u64 = 1;
const MARGIN_X: u64 = 24;
const MARGIN_Y: u64 = 24;
const ROW_GAP: u64 = 2;
const CHROME_ROWS: usize = 3;
const DWELL_TICKS: u64 = 200;
const TICK_PROBE_LIMIT: usize = 1_000_000;
const FALLBACK_SPINS_PER_TICK: usize = 25_000;
const STATUS_LINE_LEN: usize = "page 00/00 count 00000000 drop 00000000 crc 00000000".len();
const TITLE_TEXT: &str = "PythOS Evidence Terminal";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvidenceTerminalError {
    Framebuffer,
    GeometryTooSmall,
    #[cfg(test)]
    RowBufferTooSmall,
    InvalidAscii,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalGeometry {
    columns: usize,
    rows: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RowPrefix {
    Marker,
    Continuation,
}

impl RowPrefix {
    const fn len(self) -> usize {
        match self {
            Self::Marker => 2,
            Self::Continuation => 1,
        }
    }

    const fn text(self) -> &'static str {
        match self {
            Self::Marker => "> ",
            Self::Continuation => " ",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RenderedSegment<'a> {
    prefix: RowPrefix,
    text: &'a [u8],
}

impl<'a> RenderedSegment<'a> {
    #[cfg(test)]
    fn write_into(&self, out: &mut [u8]) -> Result<usize, EvidenceTerminalError> {
        let needed = self.prefix.len() + self.text.len();
        if out.len() < needed {
            return Err(EvidenceTerminalError::RowBufferTooSmall);
        }
        out[..self.prefix.len()].copy_from_slice(self.prefix.text().as_bytes());
        out[self.prefix.len()..needed].copy_from_slice(self.text);
        Ok(needed)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct StatusLine {
    bytes: [u8; STATUS_LINE_LEN],
}

impl StatusLine {
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes).expect("status line is always ASCII")
    }
}

impl core::fmt::Debug for StatusLine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<&str> for StatusLine {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[derive(Clone, Copy)]
struct TranscriptCursor<'a> {
    payload: &'a [u8],
    columns: usize,
    line_start: usize,
    line_end: usize,
    segment_offset: usize,
    line_loaded: bool,
}

impl<'a> TranscriptCursor<'a> {
    fn new(
        snapshot: &'a EvidenceLogSnapshot<'a>,
        columns: usize,
    ) -> Result<Self, EvidenceTerminalError> {
        if columns <= RowPrefix::Marker.len() {
            return Err(EvidenceTerminalError::GeometryTooSmall);
        }
        Ok(Self {
            payload: snapshot.payload,
            columns,
            line_start: 0,
            line_end: 0,
            segment_offset: 0,
            line_loaded: false,
        })
    }

    #[cfg(test)]
    fn write_next_row(&mut self, out: &mut [u8]) -> Result<Option<usize>, EvidenceTerminalError> {
        let Some(segment) = self.next_segment()? else {
            return Ok(None);
        };
        Ok(Some(segment.write_into(out)?))
    }

    fn next_segment(&mut self) -> Result<Option<RenderedSegment<'a>>, EvidenceTerminalError> {
        if self.line_start >= self.payload.len() {
            return Ok(None);
        }
        self.load_line_end();

        let prefix = if self.segment_offset == 0 {
            RowPrefix::Marker
        } else {
            RowPrefix::Continuation
        };
        let capacity = self
            .columns
            .checked_sub(prefix.len())
            .ok_or(EvidenceTerminalError::GeometryTooSmall)?;
        let segment_start = self.line_start + self.segment_offset;
        let remaining = self.line_end.saturating_sub(segment_start);
        let take = remaining.min(capacity);
        let segment_end = segment_start + take;
        let segment = RenderedSegment {
            prefix,
            text: &self.payload[segment_start..segment_end],
        };

        if segment_end >= self.line_end {
            self.advance_to_next_line();
        } else {
            self.segment_offset += take;
        }

        Ok(Some(segment))
    }

    fn load_line_end(&mut self) {
        if self.line_loaded {
            return;
        }
        let mut index = self.line_start;
        while index < self.payload.len() && self.payload[index] != b'\n' {
            index += 1;
        }
        self.line_end = index;
        self.line_loaded = true;
    }

    fn advance_to_next_line(&mut self) {
        self.line_loaded = false;
        self.segment_offset = 0;
        if self.line_end < self.payload.len() && self.payload[self.line_end] == b'\n' {
            self.line_start = self.line_end + 1;
        } else {
            self.line_start = self.line_end;
        }
    }
}

pub fn render(
    snapshot: &EvidenceLogSnapshot<'_>,
    framebuffer: &PythFramebufferInfo,
) -> Result<(), EvidenceTerminalError> {
    let geometry = terminal_geometry(framebuffer)?;
    let content_rows = geometry.rows.saturating_sub(CHROME_ROWS);
    if content_rows == 0 {
        return Err(EvidenceTerminalError::GeometryTooSmall);
    }

    let total_pages = wrapped_page_count(snapshot, geometry.columns, geometry.rows);
    let surface = framebuffer::TerminalSurface::new(framebuffer)
        .map_err(|_| EvidenceTerminalError::Framebuffer)?;
    let mut cursor = TranscriptCursor::new(snapshot, geometry.columns)?;

    for page_index in 0..total_pages {
        render_page(
            &surface,
            snapshot,
            &mut cursor,
            geometry,
            page_index + 1,
            total_pages,
        )?;
        if page_index + 1 != total_pages {
            dwell_between_pages();
        }
    }

    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn page_count(snapshot: &EvidenceLogSnapshot<'_>, terminal_rows: usize) -> usize {
    page_count_for_lines(snapshot.header.lines as usize, terminal_rows)
}

fn render_page(
    surface: &framebuffer::TerminalSurface,
    snapshot: &EvidenceLogSnapshot<'_>,
    cursor: &mut TranscriptCursor<'_>,
    geometry: TerminalGeometry,
    page_number: usize,
    page_total: usize,
) -> Result<(), EvidenceTerminalError> {
    surface.clear();
    surface
        .draw_text(MARGIN_X, row_y(0), TITLE_TEXT, TerminalTextRole::Title)
        .map_err(|_| EvidenceTerminalError::Framebuffer)?;

    let status = format_status_line(
        page_number,
        page_total,
        snapshot.header.lines,
        snapshot.header.dropped,
        snapshot.header.crc32,
    );
    surface
        .draw_text(
            MARGIN_X,
            row_y(1),
            status.as_str(),
            TerminalTextRole::Status,
        )
        .map_err(|_| EvidenceTerminalError::Framebuffer)?;

    for content_row in 0..geometry.rows.saturating_sub(CHROME_ROWS) {
        let Some(segment) = cursor.next_segment()? else {
            break;
        };
        let prefix_x = MARGIN_X;
        let text_x = prefix_x + (segment.prefix.len() as u64 * glyph_advance());
        let y = row_y(CHROME_ROWS + content_row);
        if segment.prefix == RowPrefix::Marker {
            surface
                .draw_text(prefix_x, y, segment.prefix.text(), TerminalTextRole::Body)
                .map_err(|_| EvidenceTerminalError::Framebuffer)?;
        }
        let text =
            core::str::from_utf8(segment.text).map_err(|_| EvidenceTerminalError::InvalidAscii)?;
        surface
            .draw_text(text_x, y, text, TerminalTextRole::Body)
            .map_err(|_| EvidenceTerminalError::Framebuffer)?;
    }

    Ok(())
}

fn terminal_geometry(
    framebuffer: &PythFramebufferInfo,
) -> Result<TerminalGeometry, EvidenceTerminalError> {
    framebuffer
        .validate()
        .map_err(|_| EvidenceTerminalError::Framebuffer)?;
    let inner_width = u64::from(framebuffer.width)
        .checked_sub(MARGIN_X * 2)
        .ok_or(EvidenceTerminalError::GeometryTooSmall)?;
    let inner_height = u64::from(framebuffer.height)
        .checked_sub(MARGIN_Y * 2)
        .ok_or(EvidenceTerminalError::GeometryTooSmall)?;
    let columns = inner_width / glyph_advance();
    let rows = inner_height / row_advance();
    if columns <= RowPrefix::Marker.len() as u64 || rows <= CHROME_ROWS as u64 {
        return Err(EvidenceTerminalError::GeometryTooSmall);
    }
    Ok(TerminalGeometry {
        columns: columns as usize,
        rows: rows as usize,
    })
}

fn wrapped_page_count(
    snapshot: &EvidenceLogSnapshot<'_>,
    columns: usize,
    terminal_rows: usize,
) -> usize {
    let page_rows = terminal_rows.saturating_sub(CHROME_ROWS);
    if page_rows == 0 {
        return 0;
    }
    let mut cursor = match TranscriptCursor::new(snapshot, columns) {
        Ok(cursor) => cursor,
        Err(_) => return 0,
    };
    let mut wrapped_rows = 0usize;
    while matches!(cursor.next_segment(), Ok(Some(_))) {
        wrapped_rows += 1;
    }
    if wrapped_rows == 0 {
        return 1;
    }
    wrapped_rows.div_ceil(page_rows)
}

fn page_count_for_lines(lines: usize, terminal_rows: usize) -> usize {
    let page_rows = terminal_rows.saturating_sub(CHROME_ROWS);
    if page_rows == 0 {
        return 0;
    }
    if lines == 0 {
        return 1;
    }
    lines.div_ceil(page_rows)
}

fn format_status_line(
    page_number: usize,
    page_total: usize,
    count: u32,
    dropped: u32,
    crc32: u32,
) -> StatusLine {
    let mut bytes = *b"page 00/00 count 00000000 drop 00000000 crc 00000000";
    write_dec2(&mut bytes[5..7], page_number);
    write_dec2(&mut bytes[8..10], page_total);
    write_hex8(&mut bytes[17..25], count);
    write_hex8(&mut bytes[31..39], dropped);
    write_hex8(&mut bytes[44..52], crc32);
    StatusLine { bytes }
}

fn dwell_between_pages() {
    let start = timer::ticks();
    if wait_for_tick_progress(start) {
        wait_until(start.saturating_add(DWELL_TICKS));
        return;
    }
    fallback_spin_delay();
}

fn wait_for_tick_progress(start: u64) -> bool {
    for _ in 0..TICK_PROBE_LIMIT {
        if timer::ticks() != start {
            return true;
        }
        hint::spin_loop();
    }
    false
}

fn wait_until(target_tick: u64) {
    while timer::ticks() < target_tick {
        hint::spin_loop();
    }
}

#[cfg(feature = "evidence-terminal")]
fn fallback_spin_delay() {
    // Fallback used only when PIT ticks stay stuck for one bounded probe
    // interval. This spin calibration is verified only on O2 Micro 1217:8620
    // under the `evidence-terminal` path and should not be treated as a
    // general timing guarantee for other hardware.
    for _ in 0..(DWELL_TICKS as usize * FALLBACK_SPINS_PER_TICK) {
        hint::spin_loop();
    }
}

fn row_y(row: usize) -> u64 {
    MARGIN_Y + (row as u64 * row_advance())
}

const fn glyph_advance() -> u64 {
    GLYPH_W * SCALE
}

const fn row_advance() -> u64 {
    GLYPH_H * SCALE + ROW_GAP
}

fn write_dec2(target: &mut [u8], value: usize) {
    let value = value.min(99);
    target[0] = b'0' + ((value / 10) as u8);
    target[1] = b'0' + ((value % 10) as u8);
}

fn write_hex8(target: &mut [u8], value: u32) {
    for (index, shift) in (0..32).step_by(4).rev().enumerate() {
        let digit = ((value >> shift) & 0xF) as u8;
        target[index] = if digit < 10 {
            b'0' + digit
        } else {
            b'A' + (digit - 10)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pythos_shared::boot_protocol::{PIXEL_FORMAT_RGB_RESERVED_8BIT, PythFramebufferInfo};
    use pythos_shared::evidence_log::{
        EVIDENCE_LOG_TOTAL_BYTES, append_line, initialize, snapshot,
    };
    use std::boxed::Box;
    use std::vec::Vec;

    fn snapshot_from_lines(
        lines: &[&str],
    ) -> pythos_shared::evidence_log::EvidenceLogSnapshot<'static> {
        let mut buffer = Box::new([0u8; EVIDENCE_LOG_TOTAL_BYTES]);
        initialize(&mut *buffer).unwrap();
        for line in lines {
            append_line(&mut *buffer, line).unwrap();
        }
        let leaked = Box::leak(buffer);
        snapshot(leaked).unwrap()
    }

    fn framebuffer_info(width: u32, height: u32) -> PythFramebufferInfo {
        PythFramebufferInfo {
            physical_base: 0xC000_0000,
            mapped_virtual_base: 0xFFFF_C000_0000_0000,
            byte_length: u64::from(width) * u64::from(height) * 4,
            width,
            height,
            pixels_per_scanline: width,
            pixel_format: PIXEL_FORMAT_RGB_RESERVED_8BIT,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            reserved_mask: 0,
        }
    }

    fn mapped_framebuffer_info(width: u32, height: u32) -> (Vec<u32>, PythFramebufferInfo) {
        let mut pixels = vec![0u32; (width as usize) * (height as usize)];
        let info = PythFramebufferInfo {
            physical_base: 0xC000_0000,
            mapped_virtual_base: pixels.as_mut_ptr() as u64,
            byte_length: u64::from(width) * u64::from(height) * 4,
            width,
            height,
            pixels_per_scanline: width,
            pixel_format: PIXEL_FORMAT_RGB_RESERVED_8BIT,
            red_mask: 0,
            green_mask: 0,
            blue_mask: 0,
            reserved_mask: 0,
        };
        (pixels, info)
    }

    #[test]
    fn page_count_uses_rows_remaining_after_chrome() {
        let lines = 73;
        assert_eq!(page_count_for_lines(lines, 20), 5);
    }

    #[test]
    fn page_count_uses_snapshot_line_count() {
        let snapshot = snapshot_from_lines(&["A", "B", "C", "D"]);
        assert_eq!(page_count(&snapshot, 5), 2);
    }

    #[test]
    fn status_line_formats_count_drop_and_crc_as_hex() {
        let line = format_status_line(1, 4, 242, 0, 0x8A31_C04E);
        assert_eq!(line, "page 01/04 count 000000F2 drop 00000000 crc 8A31C04E");
    }

    #[test]
    fn terminal_geometry_uses_task_constants() {
        let geometry = terminal_geometry(&framebuffer_info(800, 600)).unwrap();
        assert_eq!(geometry.columns, 94);
        assert_eq!(geometry.rows, 55);
    }

    #[test]
    fn wrapped_rows_use_space_prefix_after_first_segment() {
        let snapshot = snapshot_from_lines(&["ABCDEFGH"]);
        let mut cursor = TranscriptCursor::new(&snapshot, 6).unwrap();
        let mut row = [0u8; 6];

        let len = cursor.write_next_row(&mut row).unwrap().unwrap();
        assert_eq!(&row[..len], b"> ABCD");

        let len = cursor.write_next_row(&mut row).unwrap().unwrap();
        assert_eq!(&row[..len], b" EFGH");
    }

    #[test]
    fn wrapped_rows_paginate_after_chrome_rows() {
        let snapshot = snapshot_from_lines(&["ABCDEFGH"]);
        assert_eq!(wrapped_page_count(&snapshot, 6, 4), 2);
    }

    #[test]
    fn render_draws_single_page_terminal_to_framebuffer() {
        let snapshot = snapshot_from_lines(&["PYTHOS:CORE:FRAMEBUFFER_READY"]);
        let (pixels, info) = mapped_framebuffer_info(800, 600);

        render(&snapshot, &info).unwrap();

        assert!(pixels.iter().any(|pixel| *pixel != 0));
    }
}
