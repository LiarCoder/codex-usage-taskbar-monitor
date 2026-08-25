//! Taskbar positioning helpers.

use windows::Win32::Foundation::{HWND, POINT, RECT};

use crate::platform::native::{self, TaskbarWindow};

use super::layout::total_widget_width;

pub(crate) fn taskbar_at_point(pt: POINT) -> Option<(usize, TaskbarWindow)> {
    native::find_taskbars()
        .into_iter()
        .enumerate()
        .find(|(_, taskbar)| {
            pt.x >= taskbar.rect.left
                && pt.x < taskbar.rect.right
                && pt.y >= taskbar.rect.top
                && pt.y < taskbar.rect.bottom
        })
}

pub(crate) fn tray_left_for_taskbar(taskbar_hwnd: HWND, taskbar_rect: RECT) -> i32 {
    let mut tray_left = taskbar_rect.right;
    if let Some(tray_hwnd) = native::find_child_window(taskbar_hwnd, "TrayNotifyWnd") {
        if let Some(tray_rect) = native::get_window_rect_safe(tray_hwnd) {
            tray_left = tray_rect.left;
        }
    }
    tray_left
}

pub(crate) fn effective_tray_offset(
    tray_left: i32,
    taskbar_left: i32,
    widget_width: i32,
    desired_offset: i32,
) -> i32 {
    let max_offset = (tray_left - taskbar_left - widget_width).max(0);
    desired_offset.clamp(0, max_offset)
}

pub(crate) fn clamp_offset_for_taskbar(taskbar_hwnd: HWND, taskbar_rect: RECT, offset: i32) -> i32 {
    let tray_left = tray_left_for_taskbar(taskbar_hwnd, taskbar_rect);
    effective_tray_offset(tray_left, taskbar_rect.left, total_widget_width(), offset)
}

pub(crate) fn offset_for_drop_point(
    taskbar_hwnd: HWND,
    taskbar_rect: RECT,
    pt: POINT,
    drag_start_client_x: i32,
) -> i32 {
    let tray_left = tray_left_for_taskbar(taskbar_hwnd, taskbar_rect);
    let desired_left = pt.x - taskbar_rect.left - drag_start_client_x;
    let offset = tray_left - taskbar_rect.left - total_widget_width() - desired_left;
    clamp_offset_for_taskbar(taskbar_hwnd, taskbar_rect, offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_offset_is_clamped_without_changing_requested_offset() {
        let desired_offset = 100;

        let effective_offset = effective_tray_offset(500, 0, 450, desired_offset);

        assert_eq!(effective_offset, 50);
        assert_eq!(desired_offset, 100);
    }

    #[test]
    fn effective_offset_preserves_requested_offset_when_space_is_available() {
        assert_eq!(effective_tray_offset(700, 0, 450, 100), 100);
    }
}
