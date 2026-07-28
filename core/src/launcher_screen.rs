//! ADR 0053 kernel-mode interactive launcher event loop.
//!
//! Runs entirely in kernel mode, on the kernel's own CR3, strictly *before*
//! the existing one-shot ring-3 entry
//! (`user_mode::enter_persistent_user_process` -> `ring3_enter_forever_abi`
//! does `iretq` and is `-> !`, never returning to Rust except via fault
//! handling, which terminates rather than resumes). This module cannot be a
//! callback reachable from inside or after that call — it must finish and
//! return before `normal_boot::run` ever activates the shell's address space.
// Only used from `normal_boot.rs` (ADR 0053, Task D wiring below), which is
// compiled out entirely under `--features verify`. `tile_contains`/`clamp`
// are exercised directly by this file's own tests; the hardware-touching
// `run_until_click` is not (Task E proves that live, via QEMU/QMP).
#![cfg_attr(any(test, feature = "verify"), allow(dead_code))]

use crate::framebuffer::{
    self, LAUNCHER_TILE_HEIGHT, LAUNCHER_TILE_WIDTH, LAUNCHER_TILE_X, LAUNCHER_TILE_Y,
};
use crate::input_drivers::RawInputEvent;
use crate::ps2;
use crate::serial;
use pythos_shared::boot_protocol::PythFramebufferInfo;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LauncherError {
    Render,
}

/// Busy-poll `ps2::poll_event()` until a left-click lands inside the
/// launcher tile, redrawing the cursor on every drained mouse-move event.
/// This is a temporary, CPU-consuming pattern accepted for this slice — the
/// same class of exception the object-shell plan already granted COM2's
/// busy-polling transport.
pub fn run_until_click(framebuffer: &PythFramebufferInfo) -> Result<(), LauncherError> {
    let width = i64::from(framebuffer.width);
    let height = i64::from(framebuffer.height);
    let mut cursor_x: i64 = 0;
    let mut cursor_y: i64 = 0;
    loop {
        match ps2::poll_event() {
            Some(RawInputEvent::MouseMoved { dx, dy }) => {
                cursor_x = clamp(cursor_x + i64::from(dx), 0, width.saturating_sub(1));
                cursor_y = clamp(cursor_y + i64::from(dy), 0, height.saturating_sub(1));
                framebuffer::render_launcher_screen(framebuffer, cursor_x as u64, cursor_y as u64)
                    .map_err(|_| LauncherError::Render)?;
            }
            Some(RawInputEvent::MouseButton { left: true }) => {
                if tile_contains(cursor_x as u64, cursor_y as u64) {
                    serial::write_line("PYTHOS:CORE:LAUNCHER:CLICK_CONFIRMED");
                    return Ok(());
                }
            }
            // Key-down events and left-button-up transitions don't affect
            // the launcher's only interaction (a left click on the tile).
            Some(RawInputEvent::MouseButton { left: false })
            | Some(RawInputEvent::KeyPressed { .. })
            | None => {}
        }
        core::hint::spin_loop();
    }
}

fn clamp(value: i64, min: i64, max: i64) -> i64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Checked-add-safe rect containment against the launcher tile's fixed
/// geometry, modeled on `window_interaction.rs`'s `WindowSlot::contains` —
/// implemented locally rather than reusing that type directly, since it
/// couples to `DrawableObject`/`ObjectId`, machinery this one static tile
/// doesn't need.
fn tile_contains(x: u64, y: u64) -> bool {
    let Some(x_end) = LAUNCHER_TILE_X.checked_add(LAUNCHER_TILE_WIDTH) else {
        return false;
    };
    let Some(y_end) = LAUNCHER_TILE_Y.checked_add(LAUNCHER_TILE_HEIGHT) else {
        return false;
    };
    x >= LAUNCHER_TILE_X && x < x_end && y >= LAUNCHER_TILE_Y && y < y_end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_contains_matches_its_geometry_exactly() {
        assert!(tile_contains(LAUNCHER_TILE_X, LAUNCHER_TILE_Y));
        assert!(tile_contains(
            LAUNCHER_TILE_X + LAUNCHER_TILE_WIDTH - 1,
            LAUNCHER_TILE_Y + LAUNCHER_TILE_HEIGHT - 1
        ));
        assert!(!tile_contains(LAUNCHER_TILE_X - 1, LAUNCHER_TILE_Y));
        assert!(!tile_contains(
            LAUNCHER_TILE_X,
            LAUNCHER_TILE_Y + LAUNCHER_TILE_HEIGHT
        ));
        assert!(!tile_contains(
            LAUNCHER_TILE_X + LAUNCHER_TILE_WIDTH,
            LAUNCHER_TILE_Y
        ));
    }

    #[test]
    fn clamp_bounds_a_value_into_range() {
        assert_eq!(clamp(-5, 0, 100), 0);
        assert_eq!(clamp(200, 0, 100), 100);
        assert_eq!(clamp(50, 0, 100), 50);
        assert_eq!(clamp(0, 0, 100), 0);
        assert_eq!(clamp(100, 0, 100), 100);
    }
}
