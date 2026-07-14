//! Win32 message dispatch and interaction handling.

use super::*;

/// Main window procedure
pub(super) unsafe fn dispatch(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            // For non-embedded fallback, paint normally
            let embedded = {
                let state = lock_state();
                state.as_ref().map(|s| s.embedded).unwrap_or(false)
            };
            if embedded {
                // Layered windows don't use WM_PAINT; just validate the region
                let mut ps = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut ps);
                let _ = EndPaint(hwnd, &ps);
            } else {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                paint(hdc, hwnd);
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_DISPLAYCHANGE | WM_DPICHANGED_MSG | WM_SETTINGCHANGE => {
            if msg == WM_DPICHANGED_MSG {
                let new_dpi = (wparam.0 & 0xFFFF) as u32;
                CURRENT_DPI.store(new_dpi, Ordering::Relaxed);
            }
            if msg == WM_SETTINGCHANGE {
                check_theme_change();
                check_language_change();
            }
            refresh_dpi();
            position_at_taskbar();
            render_layered();
            LRESULT(0)
        }
        WM_TIMER => handle_timer(hwnd, wparam),
        WM_APP_USAGE_UPDATED => handle_usage_updated(hwnd),
        WM_APP_UPDATE_CHECK_COMPLETE => handle_update_check_complete(hwnd),
        WM_SETCURSOR => handle_set_cursor(hwnd, msg, wparam, lparam),
        WM_LBUTTONDOWN => handle_left_button_down(hwnd, lparam),
        WM_MOUSEMOVE => handle_mouse_move(),
        WM_LBUTTONUP => handle_left_button_up(hwnd),
        WM_RBUTTONUP => handle_right_button_up(hwnd),
        WM_COMMAND => handle_command(hwnd, wparam),
        _ if msg == WM_APP_TRAY => handle_tray_message(hwnd, lparam),
        WM_DESTROY => handle_destroy(hwnd),
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn handle_timer(hwnd: HWND, wparam: WPARAM) -> LRESULT {
    let timer_id = wparam.0;
    match timer_id {
        TIMER_POLL => {
            let watch = {
                let state = lock_state();
                state
                    .as_ref()
                    .map(|s| (s.auth_error_paused_polling, s.auth_watch_snapshot.clone()))
            };
            match watch {
                Some((true, previous_snapshot)) => {
                    let current_snapshot = poller::credential_watch_snapshot();
                    if current_snapshot != previous_snapshot {
                        let mut state = lock_state();
                        if let Some(s) = state.as_mut() {
                            if s.auth_error_paused_polling {
                                s.auth_watch_snapshot = current_snapshot;
                            }
                        }
                        drop(state);
                        let sh = SendHwnd::from_hwnd(hwnd);
                        std::thread::spawn(move || {
                            do_poll(sh);
                        });
                    }
                }
                Some((false, _)) => {
                    let sh = SendHwnd::from_hwnd(hwnd);
                    std::thread::spawn(move || {
                        do_poll(sh);
                    });
                }
                None => {}
            }
        }
        TIMER_COUNTDOWN => {
            update_display();
            render_layered();
            schedule_countdown_timer();
        }
        TIMER_RESET_POLL => {
            let should_poll = {
                let state = lock_state();
                state
                    .as_ref()
                    .map(|s| !s.auth_error_paused_polling)
                    .unwrap_or(false)
            };
            if should_poll {
                let sh = SendHwnd::from_hwnd(hwnd);
                std::thread::spawn(move || {
                    do_poll(sh);
                });
            }
        }
        TIMER_UPDATE_CHECK => {
            begin_update_check(hwnd, false);
        }
        _ => {}
    }
    LRESULT(0)
}

unsafe fn handle_usage_updated(hwnd: HWND) -> LRESULT {
    check_theme_change();
    check_language_change();
    render_layered();
    schedule_countdown_timer();
    suppress_tray_reposition_for(Duration::from_millis(
        TRAY_ICON_UPDATE_REPOSITION_SUPPRESS_MS,
    ));
    sync_tray_icons(hwnd);
    LRESULT(0)
}

unsafe fn handle_update_check_complete(hwnd: HWND) -> LRESULT {
    schedule_auto_update_check(hwnd);
    LRESULT(0)
}

unsafe fn handle_set_cursor(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let is_dragging = {
        let state = lock_state();
        state.as_ref().map(|s| s.dragging).unwrap_or(false)
    };
    if is_dragging {
        let cursor = LoadCursorW(HINSTANCE::default(), IDC_SIZEWE).unwrap_or_default();
        SetCursor(cursor);
        return LRESULT(1);
    }
    if cursor_is_on_drag_handle(hwnd) {
        let cursor = LoadCursorW(HINSTANCE::default(), IDC_SIZEWE).unwrap_or_default();
        SetCursor(cursor);
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn handle_left_button_down(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    let client_x = (lparam.0 & 0xFFFF) as i16 as i32;
    let client_y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    if !is_drag_handle_point(client_x, client_y) {
        return LRESULT(0);
    }

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let mut state = lock_state();
    if let Some(s) = state.as_mut() {
        s.dragging = true;
        s.drag_start_mouse_x = pt.x;
        s.drag_start_client_x = client_x;
        s.drag_start_offset = s.tray_offset;
    }
    SetCapture(hwnd);
    LRESULT(0)
}

unsafe fn handle_mouse_move() -> LRESULT {
    let is_dragging = {
        let state = lock_state();
        state.as_ref().map(|s| s.dragging).unwrap_or(false)
    };
    if is_dragging {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let move_target = {
            let mut state = lock_state();
            let s = match state.as_mut() {
                Some(s) => s,
                None => return LRESULT(0),
            };

            // Moving mouse left = positive delta = larger offset (further left)
            let delta = s.drag_start_mouse_x - pt.x;
            let mut new_offset = s.drag_start_offset + delta;

            // Clamp: offset >= 0 (can't go right of default)
            if new_offset < 0 {
                new_offset = 0;
            }

            let taskbar_hwnd = s.taskbar_hwnd;
            let embedded = s.embedded;
            let hwnd_val = s.hwnd.to_hwnd();

            // Clamp: don't go past left edge of taskbar
            if let Some(taskbar_hwnd) = taskbar_hwnd {
                if let Some(taskbar_rect) = native::get_taskbar_rect(taskbar_hwnd) {
                    let mut tray_left = taskbar_rect.right;
                    if let Some(tray_hwnd) =
                        native::find_child_window(taskbar_hwnd, "TrayNotifyWnd")
                    {
                        if let Some(tray_rect) = native::get_window_rect_safe(tray_hwnd) {
                            tray_left = tray_rect.left;
                        }
                    }
                    let widget_width = total_widget_width_for_state(s);
                    let max_offset = (tray_left - taskbar_rect.left - widget_width).max(0);
                    if new_offset > max_offset {
                        new_offset = max_offset;
                    }

                    s.tray_offset = new_offset;

                    let taskbar_height = taskbar_rect.bottom - taskbar_rect.top;
                    let anchor_top = taskbar_rect.top;
                    let anchor_height = taskbar_height;
                    let widget_height = sc(WIDGET_HEIGHT);
                    let y = compute_anchor_y(anchor_top, anchor_height, widget_height);
                    let x = if embedded {
                        tray_left - taskbar_rect.left - widget_width - new_offset
                    } else {
                        tray_left - widget_width - new_offset
                    };
                    Some((
                        hwnd_val,
                        embedded,
                        x,
                        y,
                        taskbar_rect.top,
                        widget_width,
                        widget_height,
                    ))
                } else {
                    s.tray_offset = new_offset;
                    None
                }
            } else {
                s.tray_offset = new_offset;
                None
            }
        };

        if let Some((hwnd_val, embedded, x, y, taskbar_top, widget_width, widget_height)) =
            move_target
        {
            if embedded {
                native::move_window(hwnd_val, x, y - taskbar_top, widget_width, widget_height);
            } else {
                native::move_window(hwnd_val, x, y, widget_width, widget_height);
            }
        }
    }
    LRESULT(0)
}

unsafe fn handle_left_button_up(hwnd: HWND) -> LRESULT {
    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let drag_result = {
        let mut state = lock_state();
        if let Some(s) = state.as_mut() {
            if s.dragging {
                s.dragging = false;
                Some((s.taskbar_index, s.drag_start_client_x))
            } else {
                None
            }
        } else {
            None
        }
    };
    if let Some((current_taskbar_index, drag_start_client_x)) = drag_result {
        let _ = ReleaseCapture();
        if let Some((target_index, target_taskbar)) = taskbar_at_point(pt) {
            if target_index != current_taskbar_index {
                let new_offset = offset_for_drop_point(
                    target_taskbar.hwnd,
                    target_taskbar.rect,
                    pt,
                    drag_start_client_x,
                );
                {
                    let mut state = lock_state();
                    if let Some(s) = state.as_mut() {
                        s.tray_offset = new_offset;
                    }
                }
                if attach_to_taskbar(hwnd, target_index) {
                    position_at_taskbar();
                    render_layered();
                }
            }
        }
        save_state_settings();
    }
    LRESULT(0)
}

unsafe fn handle_right_button_up(hwnd: HWND) -> LRESULT {
    show_context_menu(hwnd);
    LRESULT(0)
}

unsafe fn handle_command(hwnd: HWND, wparam: WPARAM) -> LRESULT {
    let id = wparam.0 as u16;
    match id {
        1 => {
            {
                let mut state = lock_state();
                if let Some(s) = state.as_mut() {
                    s.session_text = "...".to_string();
                    s.weekly_text = "...".to_string();
                    s.force_notify_auth_error = true;
                }
            }
            render_layered();
            let sh = SendHwnd::from_hwnd(hwnd);
            std::thread::spawn(move || {
                do_poll(sh);
            });
        }
        IDM_VERSION_ACTION => {
            let (install_channel, release) = {
                let state = lock_state();
                match state.as_ref() {
                    Some(s) => (
                        s.install_channel,
                        match &s.update_status {
                            UpdateStatus::Available(release) => Some(release.clone()),
                            _ => None,
                        },
                    ),
                    None => (InstallChannel::Portable, None),
                }
            };

            match install_channel {
                InstallChannel::Winget => {
                    if release.is_some() {
                        begin_winget_update(hwnd);
                    } else {
                        begin_update_check(hwnd, true);
                    }
                }
                InstallChannel::Portable => {
                    if let Some(release) = release {
                        begin_update_apply(hwnd, release);
                    } else {
                        begin_update_check(hwnd, true);
                    }
                }
            }
        }
        2 => {
            let hook = {
                let state = lock_state();
                state.as_ref().and_then(|s| s.win_event_hook)
            };
            if let Some(h) = hook {
                native::unhook_win_event(h);
            }
            PostQuitMessage(0);
        }
        IDM_RESET_POSITION => {
            {
                let mut state = lock_state();
                if let Some(s) = state.as_mut() {
                    s.tray_offset = 0;
                }
            }
            save_state_settings();
            position_at_taskbar();
        }
        IDM_COMPACT_MODE => {
            {
                let mut state = lock_state();
                if let Some(s) = state.as_mut() {
                    s.compact_mode = !s.compact_mode;
                }
            }
            save_state_settings();
            position_at_taskbar();
            render_layered();
        }
        IDM_SHOW_5HOUR_WINDOW | IDM_SHOW_7DAY_WINDOW => {
            let changed = {
                let mut state = lock_state();
                if let Some(s) = state.as_mut() {
                    if id == IDM_SHOW_5HOUR_WINDOW {
                        if s.show_5hour_window && !s.show_7day_window {
                            false
                        } else {
                            s.show_5hour_window = !s.show_5hour_window;
                            true
                        }
                    } else if s.show_7day_window && !s.show_5hour_window {
                        false
                    } else {
                        s.show_7day_window = !s.show_7day_window;
                        true
                    }
                } else {
                    false
                }
            };
            if changed {
                save_state_settings();
                render_layered();
                sync_tray_icons(hwnd);
            }
        }
        IDM_START_WITH_WINDOWS => {
            set_startup_enabled(!is_startup_enabled());
        }
        IDM_FREQ_1MIN | IDM_FREQ_5MIN | IDM_FREQ_15MIN | IDM_FREQ_1HOUR => {
            let new_interval = match id {
                IDM_FREQ_1MIN => POLL_1_MIN,
                IDM_FREQ_5MIN => POLL_5_MIN,
                IDM_FREQ_15MIN => POLL_15_MIN,
                IDM_FREQ_1HOUR => POLL_1_HOUR,
                _ => POLL_15_MIN,
            };
            {
                let mut state = lock_state();
                if let Some(s) = state.as_mut() {
                    s.poll_interval_ms = new_interval;
                }
            }
            save_state_settings();
            // Reset the poll timer with the new interval
            SetTimer(hwnd, TIMER_POLL, new_interval, None);
        }
        IDM_USAGE_DISPLAY_USED | IDM_USAGE_DISPLAY_REMAINING => {
            {
                let mut state = lock_state();
                if let Some(s) = state.as_mut() {
                    s.usage_display = if id == IDM_USAGE_DISPLAY_REMAINING {
                        UsageDisplayMode::Remaining
                    } else {
                        UsageDisplayMode::Used
                    };
                    refresh_usage_texts(s);
                }
            }
            save_state_settings();
            render_layered();
            sync_tray_icons(hwnd);
        }
        IDM_LANG_SYSTEM
        | IDM_LANG_ENGLISH
        | IDM_LANG_DUTCH
        | IDM_LANG_SPANISH
        | IDM_LANG_FRENCH
        | IDM_LANG_GERMAN
        | IDM_LANG_JAPANESE
        | IDM_LANG_KOREAN
        | IDM_LANG_TRADITIONAL_CHINESE
        | IDM_LANG_SIMPLIFIED_CHINESE
        | IDM_LANG_RUSSIAN
        | IDM_LANG_PORTUGUESE_BRAZIL => {
            let language_override = match id {
                IDM_LANG_SYSTEM => None,
                IDM_LANG_ENGLISH => Some(LanguageId::English),
                IDM_LANG_DUTCH => Some(LanguageId::Dutch),
                IDM_LANG_SPANISH => Some(LanguageId::Spanish),
                IDM_LANG_FRENCH => Some(LanguageId::French),
                IDM_LANG_GERMAN => Some(LanguageId::German),
                IDM_LANG_JAPANESE => Some(LanguageId::Japanese),
                IDM_LANG_KOREAN => Some(LanguageId::Korean),
                IDM_LANG_TRADITIONAL_CHINESE => Some(LanguageId::TraditionalChinese),
                IDM_LANG_SIMPLIFIED_CHINESE => Some(LanguageId::SimplifiedChinese),
                IDM_LANG_RUSSIAN => Some(LanguageId::Russian),
                IDM_LANG_PORTUGUESE_BRAZIL => Some(LanguageId::PortugueseBrazil),
                _ => None,
            };
            {
                let mut state = lock_state();
                if let Some(s) = state.as_mut() {
                    apply_language_to_state(s, language_override);
                }
            }
            save_state_settings();
            render_layered();
        }
        id if id == tray::IDM_TOGGLE_WIDGET => {
            toggle_widget_visibility(hwnd);
        }
        _ => {}
    }
    LRESULT(0)
}

unsafe fn handle_tray_message(hwnd: HWND, lparam: LPARAM) -> LRESULT {
    match tray::handle_message(lparam) {
        tray::TrayAction::ToggleWidget => {
            toggle_widget_visibility(hwnd);
        }
        tray::TrayAction::ShowContextMenu => {
            show_context_menu(hwnd);
        }
        tray::TrayAction::Nothing => {}
    }
    LRESULT(0)
}

unsafe fn handle_destroy(hwnd: HWND) -> LRESULT {
    let hook = {
        let state = lock_state();
        state.as_ref().and_then(|s| s.win_event_hook)
    };
    if let Some(h) = hook {
        native::unhook_win_event(h);
    }
    tray::remove(hwnd);
    PostQuitMessage(0);
    LRESULT(0)
}

fn cursor_is_on_drag_handle(hwnd: HWND) -> bool {
    unsafe {
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_err() || !ScreenToClient(hwnd, &mut pt).as_bool() {
            return false;
        }
        is_drag_handle_point(pt.x, pt.y)
    }
}
