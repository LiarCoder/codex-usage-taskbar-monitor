//! Layout constants and widget dimension calculations.
//!
//! All values are designed at 96 DPI and scaled via `sc()`.

use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::UI::HiDpi::GetDpiForWindow;

use super::AppState;

/// Current system DPI (96 = 100% scaling, 144 = 150%, 192 = 200%, etc.)
pub(crate) static CURRENT_DPI: AtomicU32 = AtomicU32::new(96);

/// Scale a base pixel value (designed at 96 DPI) to the current DPI.
pub(crate) fn sc(px: i32) -> i32 {
    let dpi = CURRENT_DPI.load(Ordering::Relaxed);
    (px as f64 * dpi as f64 / 96.0).round() as i32
}

/// Re-query the monitor DPI for our window and update the cached value.
/// Uses GetDpiForWindow which returns the live DPI (unlike GetDpiForSystem
/// which is cached at process startup and never changes).
pub(crate) fn refresh_dpi() {
    let hwnd = {
        let state = super::lock_state();
        state.as_ref().map(|s| s.hwnd.to_hwnd())
    };
    if let Some(hwnd) = hwnd {
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        if dpi > 0 {
            CURRENT_DPI.store(dpi, Ordering::Relaxed);
        }
    }
}

pub(crate) const SEGMENT_W: i32 = 10;
pub(crate) const SEGMENT_H: i32 = 13;
pub(crate) const SEGMENT_GAP: i32 = 1;
pub(crate) const SEGMENT_COUNT: i32 = 10;
pub(crate) const CORNER_RADIUS: i32 = 2;

pub(crate) const LEFT_DIVIDER_W: i32 = 3;
pub(crate) const DIVIDER_RIGHT_MARGIN: i32 = 10;
pub(crate) const LABEL_WIDTH: i32 = 18;
pub(crate) const LABEL_RIGHT_MARGIN: i32 = 10;
pub(crate) const BAR_RIGHT_MARGIN: i32 = 4;
pub(crate) const TEXT_WIDTH: i32 = 62;
pub(crate) const MODEL_RIGHT_MARGIN: i32 = 3;
pub(crate) const RIGHT_MARGIN: i32 = 1;
pub(crate) const WIDGET_HEIGHT: i32 = 46;

pub(crate) fn is_drag_handle_point(client_x: i32, client_y: i32) -> bool {
    let divider_h = sc(25);
    let divider_top = (sc(WIDGET_HEIGHT) - divider_h) / 2;
    client_x >= 0
        && client_x < sc(LEFT_DIVIDER_W)
        && client_y >= divider_top
        && client_y < divider_top + divider_h
}

pub(crate) fn row_bar_segment_count(_active_models: i32) -> i32 {
    SEGMENT_COUNT
}

pub(crate) fn total_widget_width_for(active_models: i32, compact_mode: bool) -> i32 {
    let bar_segments = row_bar_segment_count(active_models);
    let model_width = model_usage_width(bar_segments, compact_mode);

    sc(LEFT_DIVIDER_W)
        + sc(DIVIDER_RIGHT_MARGIN)
        + sc(LABEL_WIDTH)
        + sc(LABEL_RIGHT_MARGIN)
        + model_width * active_models
        + sc(MODEL_RIGHT_MARGIN) * (active_models - 1)
        + sc(RIGHT_MARGIN)
}

pub(crate) fn total_widget_width_for_state(state: &AppState) -> i32 {
    total_widget_width_for(1 /* codex-only */, state.compact_mode)
}

pub(crate) fn total_widget_width() -> i32 {
    let (active_models, compact_mode) = {
        let state = super::lock_state();
        state
            .as_ref()
            .map(|s| {
                (
                    1, // codex-only
                    s.compact_mode,
                )
            })
            .unwrap_or((1, false))
    };
    total_widget_width_for(active_models, compact_mode)
}

pub(crate) fn model_usage_width(segment_count: i32, compact_mode: bool) -> i32 {
    if compact_mode {
        sc(TEXT_WIDTH)
    } else {
        (sc(SEGMENT_W) + sc(SEGMENT_GAP)) * segment_count - sc(SEGMENT_GAP)
            + sc(BAR_RIGHT_MARGIN)
            + sc(TEXT_WIDTH)
    }
}

/// Compute the vertical anchor for the widget relative to the taskbar.
pub(crate) fn compute_anchor_y(anchor_top: i32, anchor_height: i32, widget_height: i32) -> i32 {
    let anchor_bottom = anchor_top + anchor_height;
    (anchor_bottom - widget_height).max(anchor_top)
}
