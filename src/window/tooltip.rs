use std::time::SystemTime;

use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, TOOLTIPS_CLASSW, TTF_ABSOLUTE,
    TTF_TRACK, TTM_ACTIVATE, TTM_ADDTOOLW, TTM_NEWTOOLRECTW, TTM_POP, TTM_SETMAXTIPWIDTH,
    TTM_TRACKACTIVATE, TTM_TRACKPOSITION, TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX,
    TTTOOLINFOW,
};

use super::*;
use crate::radar::{self as radar_data, Recommendation};

const RADAR_TOOLTIP_ID: usize = 1;
const RADAR_TOOLTIP_DELAY_MS: u32 = 500;
const RADAR_TOOLTIP_GAP: i32 = 4;
const RADAR_TOOLTIP_MAX_WIDTH: i32 = 480;
const TTTOOLINFOW_V2_SIZE: u32 = std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32;

pub(super) fn initialize_radar_tooltip(hwnd: HWND) {
    unsafe {
        let controls = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_WIN95_CLASSES,
        };
        if !InitCommonControlsEx(&controls).as_bool() {
            diagnose::log("unable to initialize common controls for CodexRadar tooltip");
            return;
        }

        let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap();
        let tooltip = match CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            TOOLTIPS_CLASSW,
            PCWSTR::null(),
            WINDOW_STYLE(WS_POPUP.0 | TTS_ALWAYSTIP | TTS_NOPREFIX),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            hwnd,
            HMENU::default(),
            HINSTANCE(hinstance.0),
            None,
        ) {
            Ok(tooltip) => tooltip,
            Err(error) => {
                diagnose::log_error("unable to create CodexRadar tooltip", error);
                return;
            }
        };

        let (text_pointer, enabled) = {
            let mut state = lock_state();
            let Some(app_state) = state.as_mut() else {
                let _ = DestroyWindow(tooltip);
                return;
            };
            app_state.radar_tooltip_hwnd = Some(tooltip);
            app_state.radar_tooltip_text =
                native::wide_str(&format_widget_tooltip(app_state, now_unix_secs()));
            (
                PWSTR(app_state.radar_tooltip_text.as_mut_ptr()),
                widget_tooltip_available(app_state),
            )
        };

        let mut tool = tooltip_tool_info(hwnd, text_pointer);
        let added = SendMessageW(
            tooltip,
            TTM_ADDTOOLW,
            WPARAM(0),
            LPARAM(&mut tool as *mut _ as isize),
        );
        if added.0 == 0 {
            diagnose::log("unable to register CodexRadar tooltip tool");
            let mut state = lock_state();
            if let Some(app_state) = state.as_mut() {
                app_state.radar_tooltip_hwnd = None;
            }
            let _ = DestroyWindow(tooltip);
            return;
        }
        let _ = SendMessageW(
            tooltip,
            TTM_SETMAXTIPWIDTH,
            WPARAM(0),
            LPARAM(sc(RADAR_TOOLTIP_MAX_WIDTH) as isize),
        );
        let _ = SendMessageW(
            tooltip,
            TTM_ACTIVATE,
            WPARAM(usize::from(enabled)),
            LPARAM(0),
        );
    }
}

pub(super) fn sync_radar_tooltip(hwnd: HWND) {
    sync_radar_tooltip_layout(hwnd);
    refresh_radar_tooltip_text(hwnd);
}

pub(super) fn sync_radar_tooltip_layout(hwnd: HWND) {
    let tooltip = {
        let state = lock_state();
        state
            .as_ref()
            .and_then(|app_state| app_state.radar_tooltip_hwnd)
    };
    let Some(tooltip) = tooltip else {
        return;
    };

    let mut tool = tooltip_tool_info(hwnd, PWSTR::null());
    unsafe {
        let _ = SendMessageW(
            tooltip,
            TTM_NEWTOOLRECTW,
            WPARAM(0),
            LPARAM(&mut tool as *mut _ as isize),
        );
        let _ = SendMessageW(
            tooltip,
            TTM_SETMAXTIPWIDTH,
            WPARAM(0),
            LPARAM(sc(RADAR_TOOLTIP_MAX_WIDTH) as isize),
        );
    }
}

pub(super) fn refresh_radar_tooltip_text(hwnd: HWND) {
    let now = now_unix_secs();
    let update = {
        let mut state = lock_state();
        let Some(app_state) = state.as_mut() else {
            return;
        };
        let Some(tooltip) = app_state.radar_tooltip_hwnd else {
            return;
        };
        let enabled = widget_tooltip_available(app_state);
        let text = native::wide_str(&format_widget_tooltip(app_state, now));
        let changed = text != app_state.radar_tooltip_text;
        if changed {
            app_state.radar_tooltip_text = text;
        }
        (
            tooltip,
            enabled,
            changed.then_some(PWSTR(app_state.radar_tooltip_text.as_mut_ptr())),
        )
    };

    unsafe {
        if let Some(text_pointer) = update.2 {
            let mut tool = tooltip_tool_info(hwnd, text_pointer);
            let _ = SendMessageW(
                update.0,
                TTM_UPDATETIPTEXTW,
                WPARAM(0),
                LPARAM(&mut tool as *mut _ as isize),
            );
        }
        let _ = SendMessageW(
            update.0,
            TTM_ACTIVATE,
            WPARAM(usize::from(update.1)),
            LPARAM(0),
        );
    }
}

pub(super) fn update_radar_tooltip_hover(hwnd: HWND, client_x: i32, client_y: i32) {
    let tooltip = {
        let state = lock_state();
        state.as_ref().and_then(|app_state| {
            (widget_tooltip_available(app_state)
                && app_state.widget_visible
                && !app_state.dragging
                && client_x >= sc(LEFT_DIVIDER_W)
                && client_x < total_widget_width_for_state(app_state)
                && client_y >= 0
                && client_y < sc(WIDGET_HEIGHT))
            .then_some(app_state.radar_tooltip_hwnd)
            .flatten()
        })
    };
    let Some(tooltip) = tooltip else {
        pop_radar_tooltip();
        return;
    };

    unsafe {
        if IsWindowVisible(tooltip).as_bool() {
            return;
        }
    }

    let should_start = {
        let mut state = lock_state();
        state.as_mut().is_some_and(|app_state| {
            if app_state.radar_tooltip_hover_pending {
                false
            } else {
                app_state.radar_tooltip_hover_pending = true;
                true
            }
        })
    };
    if !should_start {
        return;
    }

    unsafe {
        let mut tracking = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            dwHoverTime: 0,
        };
        let _ = TrackMouseEvent(&mut tracking);
        if SetTimer(hwnd, TIMER_RADAR_TOOLTIP, RADAR_TOOLTIP_DELAY_MS, None) == 0 {
            let mut state = lock_state();
            if let Some(app_state) = state.as_mut() {
                app_state.radar_tooltip_hover_pending = false;
            }
        }
    }
}

pub(super) fn show_radar_tooltip(hwnd: HWND) {
    {
        let mut state = lock_state();
        if let Some(app_state) = state.as_mut() {
            app_state.radar_tooltip_hover_pending = false;
        }
    }
    unsafe {
        let _ = KillTimer(hwnd, TIMER_RADAR_TOOLTIP);
    }
    let mut cursor = POINT::default();
    unsafe {
        if GetCursorPos(&mut cursor).is_err() {
            pop_radar_tooltip();
            return;
        }
    }
    let mut client = cursor;
    unsafe {
        if !ScreenToClient(hwnd, &mut client).as_bool() {
            pop_radar_tooltip();
            return;
        }
    }

    let tooltip = {
        let state = lock_state();
        state.as_ref().and_then(|app_state| {
            (widget_tooltip_available(app_state)
                && app_state.widget_visible
                && !app_state.dragging
                && client.x >= sc(LEFT_DIVIDER_W)
                && client.x < total_widget_width_for_state(app_state)
                && client.y >= 0
                && client.y < sc(WIDGET_HEIGHT))
            .then_some(app_state.radar_tooltip_hwnd)
            .flatten()
        })
    };
    let Some(tooltip) = tooltip else {
        pop_radar_tooltip();
        return;
    };

    refresh_radar_tooltip_text(hwnd);
    let mut tool = tooltip_tool_info(hwnd, PWSTR::null());
    park_radar_tooltip_offscreen(hwnd, tooltip);
    unsafe {
        let _ = SendMessageW(
            tooltip,
            TTM_TRACKACTIVATE,
            WPARAM(1),
            LPARAM(&mut tool as *mut _ as isize),
        );
    }
    position_radar_tooltip(hwnd, tooltip);
}

pub(super) fn pop_radar_tooltip() {
    let target = {
        let mut state = lock_state();
        state.as_mut().map(|app_state| {
            app_state.radar_tooltip_hover_pending = false;
            (app_state.hwnd.to_hwnd(), app_state.radar_tooltip_hwnd)
        })
    };
    if let Some((hwnd, tooltip)) = target {
        unsafe {
            let _ = KillTimer(hwnd, TIMER_RADAR_TOOLTIP);
            if let Some(tooltip) = tooltip {
                let mut tool = tooltip_tool_info(hwnd, PWSTR::null());
                let _ = SendMessageW(
                    tooltip,
                    TTM_TRACKACTIVATE,
                    WPARAM(0),
                    LPARAM(&mut tool as *mut _ as isize),
                );
                let _ = SendMessageW(tooltip, TTM_POP, WPARAM(0), LPARAM(0));
            }
        }
    }
}

pub(super) fn destroy_radar_tooltip() {
    let tooltip = {
        let mut state = lock_state();
        state
            .as_mut()
            .and_then(|app_state| app_state.radar_tooltip_hwnd.take())
    };
    if let Some(tooltip) = tooltip {
        unsafe {
            let _ = DestroyWindow(tooltip);
        }
    }
}

fn tooltip_tool_info(hwnd: HWND, text: PWSTR) -> TTTOOLINFOW {
    TTTOOLINFOW {
        cbSize: TTTOOLINFOW_V2_SIZE,
        uFlags: TTF_TRACK | TTF_ABSOLUTE,
        hwnd,
        uId: RADAR_TOOLTIP_ID,
        rect: RECT {
            left: sc(LEFT_DIVIDER_W),
            top: 0,
            right: total_widget_width(),
            bottom: sc(WIDGET_HEIGHT),
        },
        lpszText: text,
        ..Default::default()
    }
}

fn widget_and_monitor_rect(hwnd: HWND) -> Option<(RECT, RECT)> {
    let widget_rect = native::get_window_rect_safe(hwnd)?;
    let widget_center = POINT {
        x: widget_rect.left + (widget_rect.right - widget_rect.left) / 2,
        y: widget_rect.top + (widget_rect.bottom - widget_rect.top) / 2,
    };
    let monitor = unsafe { MonitorFromPoint(widget_center, MONITOR_DEFAULTTONEAREST) };
    let mut monitor_info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut monitor_info).as_bool() } {
        return None;
    }
    Some((widget_rect, monitor_info.rcMonitor))
}

fn park_radar_tooltip_offscreen(hwnd: HWND, tooltip: HWND) {
    let Some((_, monitor_rect)) = widget_and_monitor_rect(hwnd) else {
        return;
    };
    set_radar_tooltip_position(
        tooltip,
        POINT {
            x: monitor_rect.right,
            y: monitor_rect.bottom,
        },
    );
}

fn position_radar_tooltip(hwnd: HWND, tooltip: HWND) {
    let Some((widget_rect, monitor_rect)) = widget_and_monitor_rect(hwnd) else {
        return;
    };
    let Some(tooltip_rect) = native::get_window_rect_safe(tooltip) else {
        return;
    };
    let width = tooltip_rect.right - tooltip_rect.left;
    let height = tooltip_rect.bottom - tooltip_rect.top;
    if width <= 0 || height <= 0 {
        return;
    }
    let point = centered_tooltip_position(
        widget_rect,
        width,
        height,
        monitor_rect,
        sc(RADAR_TOOLTIP_GAP),
    );
    set_radar_tooltip_position(tooltip, point);
}

fn set_radar_tooltip_position(tooltip: HWND, point: POINT) {
    let position = ((point.y as u16 as u32) << 16) | point.x as u16 as u32;
    unsafe {
        let _ = SendMessageW(
            tooltip,
            TTM_TRACKPOSITION,
            WPARAM(0),
            LPARAM(position as isize),
        );
    }
}

fn centered_tooltip_position(
    widget: RECT,
    width: i32,
    height: i32,
    monitor: RECT,
    gap: i32,
) -> POINT {
    let x = widget.left + (widget.right - widget.left - width) / 2;
    let above = widget.top - height - gap;
    let y = if above >= monitor.top {
        above
    } else {
        widget.bottom + gap
    };

    POINT {
        x: x.clamp(monitor.left, (monitor.right - width).max(monitor.left)),
        y: y.clamp(monitor.top, (monitor.bottom - height).max(monitor.top)),
    }
}

fn widget_tooltip_available(app_state: &AppState) -> bool {
    app_state.codex_radar_enabled || has_usage_reset_time(app_state)
}

fn has_usage_reset_time(app_state: &AppState) -> bool {
    if !app_state.last_poll_ok {
        return false;
    }
    let Some(data) = app_state.data.as_ref() else {
        return false;
    };
    app_state.show_5hour_window
        && data
            .session
            .as_ref()
            .is_some_and(|section| section.resets_at.is_some())
}

fn format_widget_tooltip(app_state: &AppState, now_unix: u64) -> String {
    let usage = format_usage_reset_lines(app_state);
    let mut sections = Vec::new();
    if !usage.is_empty() {
        sections.push(usage.join("\r\n"));
    }
    let radar = format_radar_tooltip(app_state, now_unix);
    if !radar.is_empty() {
        sections.push(radar);
    }
    sections.join("\r\n\r\n")
}

fn format_usage_reset_lines(app_state: &AppState) -> Vec<String> {
    if !app_state.last_poll_ok {
        return Vec::new();
    }
    let Some(data) = app_state.data.as_ref() else {
        return Vec::new();
    };
    let strings = app_state.language.strings();
    let mut lines = Vec::new();
    if app_state.show_5hour_window {
        if let Some(line) = data
            .session
            .as_ref()
            .and_then(|section| format_usage_reset_line(strings.session_window, section.resets_at))
        {
            lines.push(line);
        }
    }
    lines
}

fn format_usage_reset_line(window: &str, resets_at: Option<SystemTime>) -> Option<String> {
    let reset_time = native::format_local_date_time(resets_at?)?;
    Some(format!("{window} → {reset_time}"))
}

fn format_radar_tooltip(app_state: &AppState, now_unix: u64) -> String {
    if !app_state.codex_radar_enabled {
        return String::new();
    }

    let strings = app_state.language.strings();
    let cache = &app_state.radar.cache;
    if !cache.has_any_data() {
        return if app_state.radar.status == RadarStatus::Loading {
            strings.radar_loading.to_string()
        } else {
            strings.radar_fetch_error.to_string()
        };
    }

    let oldest_update = [
        cache.radar.as_ref().map(|source| source.validated_at_unix),
        cache
            .computed
            .as_ref()
            .map(|source| source.validated_at_unix),
    ]
    .into_iter()
    .flatten()
    .min()
    .unwrap_or(now_unix);
    let age = format_radar_age(now_unix.saturating_sub(oldest_update), strings);
    let mut lines = vec![format_radar_tooltip_header(
        strings.radar_tooltip_header,
        &age,
    )];
    if app_state.radar.displaying_cached_data || !cache.last_refresh_complete {
        lines[0].push_str(" · ");
        lines[0].push_str(strings.radar_cached_warning);
    }

    let radar = cache.radar.as_ref().map(|source| &source.value);
    if let Some(recommendations) = radar {
        let has_slot_recommendations =
            recommendations.speed.is_some() || recommendations.smart.is_some();
        if has_slot_recommendations {
            lines.push(format_recommendation_line(
                strings.radar_speed_recommendation,
                recommendations.speed.as_ref(),
                strings.radar_data_unavailable,
                strings.minute_suffix,
            ));
            lines.push(format_recommendation_line(
                strings.radar_smart_recommendation,
                recommendations.smart.as_ref(),
                strings.radar_data_unavailable,
                strings.minute_suffix,
            ));
        }
        if !recommendations.daily_development.is_empty() {
            lines.push(format_recommendations_line(
                strings.radar_community_daily,
                &recommendations.daily_development,
                strings.minute_suffix,
            ));
        }
        if !has_slot_recommendations && recommendations.daily_development.is_empty() {
            lines.push(format_unavailable_line(
                strings.radar_speed_recommendation,
                strings.radar_data_unavailable,
            ));
            lines.push(format_unavailable_line(
                strings.radar_smart_recommendation,
                strings.radar_data_unavailable,
            ));
        }
    } else {
        lines.push(format_unavailable_line(
            strings.radar_speed_recommendation,
            strings.radar_data_unavailable,
        ));
        lines.push(format_unavailable_line(
            strings.radar_smart_recommendation,
            strings.radar_data_unavailable,
        ));
    }

    if let Some(computed) = cache.computed.as_ref() {
        lines.push(format_recommendation_line(
            strings.radar_daily_recommendation,
            computed.value.daily.as_ref(),
            strings.radar_no_eligible,
            strings.minute_suffix,
        ));
        lines.push(format_recommendation_line(
            strings.radar_intelligence_weighted,
            computed.value.intelligence_weighted.as_ref(),
            strings.radar_no_eligible,
            strings.minute_suffix,
        ));
    } else {
        lines.push(format_unavailable_line(
            strings.radar_daily_recommendation,
            strings.radar_data_unavailable,
        ));
        lines.push(format_unavailable_line(
            strings.radar_intelligence_weighted,
            strings.radar_data_unavailable,
        ));
    }

    lines.join("\r\n")
}

fn format_radar_tooltip_header(template: &str, age: &str) -> String {
    let header = template.replace("{age}", age);
    let Some((summary, community_note)) = header.rsplit_once(" · ") else {
        return header;
    };
    format!("{summary}\r\n{community_note}")
}

fn format_recommendation_line(
    label: &str,
    recommendation: Option<&Recommendation>,
    unavailable: &str,
    minute_suffix: &str,
) -> String {
    let Some(recommendation) = recommendation else {
        return format_unavailable_line(label, unavailable);
    };
    format!(
        "◆ {label}\r\n{}",
        format_recommendation_value(recommendation, minute_suffix)
    )
}

fn format_recommendations_line(
    label: &str,
    recommendations: &[Recommendation],
    minute_suffix: &str,
) -> String {
    let values = recommendations
        .iter()
        .map(|recommendation| format_recommendation_value(recommendation, minute_suffix))
        .collect::<Vec<_>>()
        .join("\r\n");
    format!("◆ {label}\r\n{values}")
}

fn format_recommendation_value(recommendation: &Recommendation, minute_suffix: &str) -> String {
    let mut value = format!(
        "  【{} {}】 · IQ {:.1} · ${:.2}",
        radar_data::model_display_name(&recommendation.model),
        recommendation.effort,
        recommendation.iq,
        recommendation.average_cost_usd
    );
    if let Some(average_minutes) = recommendation.average_minutes {
        value.push_str(&format!(" · {average_minutes:.1}{minute_suffix}"));
    }
    value
}

fn format_unavailable_line(label: &str, unavailable: &str) -> String {
    format!("◆ {label}\r\n  {unavailable}")
}

fn format_radar_age(elapsed_secs: u64, strings: Strings) -> String {
    if elapsed_secs < 60 {
        format!("1{}", strings.minute_suffix)
    } else if elapsed_secs < 60 * 60 {
        format!("{}{}", elapsed_secs / 60, strings.minute_suffix)
    } else {
        format!("{}{}", elapsed_secs / (60 * 60), strings.hour_suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radar::CachedSource;

    fn recommendation(model: &str, effort: &str, iq: f64, cost: f64) -> Recommendation {
        Recommendation {
            model: model.to_string(),
            effort: effort.to_string(),
            iq,
            average_cost_usd: cost,
            average_minutes: None,
            passed_tasks: None,
            valid_tasks: 112,
        }
    }

    fn recommendation_with_metrics(
        model: &str,
        effort: &str,
        iq: f64,
        cost: f64,
        minutes: f64,
    ) -> Recommendation {
        let mut value = recommendation(model, effort, iq, cost);
        value.average_minutes = Some(minutes);
        value.passed_tasks = Some(100);
        value
    }

    fn state(
        language: LanguageId,
        status: RadarStatus,
        cache: crate::radar::RadarCache,
    ) -> AppState {
        AppState {
            hwnd: SendHwnd::from_hwnd(HWND::default()),
            taskbar_hwnd: None,
            tray_notify_hwnd: None,
            win_event_hook: None,
            is_dark: false,
            embedded: false,
            language_override: Some(language),
            language,
            install_channel: InstallChannel::Portable,
            session_percent: 0.0,
            session_text: String::new(),
            session_available: false,
            weekly_percent: 0.0,
            weekly_text: String::new(),
            weekly_available: false,
            usage_display: UsageDisplayMode::Used,
            data: None,
            poll_interval_ms: POLL_15_MIN,
            retry_count: 0,
            force_notify_auth_error: false,
            auth_error_paused_polling: false,
            auth_watch_snapshot: String::new(),
            last_poll_ok: false,
            update_status: UpdateStatus::Idle,
            last_update_check_unix: None,
            taskbar_index: 0,
            tray_offset: 0,
            dragging: false,
            drag_start_mouse_x: 0,
            drag_start_client_x: 0,
            drag_start_offset: 0,
            widget_click_pending: false,
            widget_click_sequence: WidgetClickSequence::default(),
            widget_visible: true,
            compact_mode: false,
            show_5hour_window: true,
            show_7day_window: true,
            codex_radar_enabled: true,
            codex_radar_consent_version: 1,
            radar: RadarRuntimeState {
                status,
                cache,
                displaying_cached_data: false,
                in_flight: false,
                request_generation: 0,
            },
            radar_tooltip_hwnd: None,
            radar_tooltip_text: vec![0],
            radar_tooltip_hover_pending: false,
        }
    }

    fn full_cache() -> crate::radar::RadarCache {
        let mut cache = crate::radar::RadarCache::default();
        cache.radar = Some(CachedSource {
            value: crate::radar::RadarRecommendations {
                speed: Some(recommendation("gpt-5.6-sol", "medium", 91.07, 3.742)),
                smart: Some(recommendation("gpt-5.5", "xhigh", 100.45, 5.737)),
                daily_development: Vec::new(),
            },
            validated_at_unix: 100,
            validator: None,
        });
        cache.computed = Some(CachedSource {
            value: crate::radar::ComputedRecommendations {
                daily: Some(recommendation_with_metrics(
                    "gpt-5.6-terra",
                    "max",
                    93.75,
                    4.70,
                    20.0,
                )),
                intelligence_weighted: Some(recommendation_with_metrics(
                    "gpt-5.6-sol",
                    "xhigh",
                    105.8,
                    6.19,
                    30.0,
                )),
            },
            validated_at_unix: 100,
            validator: None,
        });
        cache.last_refresh_complete = true;
        cache
    }

    #[test]
    fn shows_5hour_reset_time_without_requiring_codex_radar() {
        let mut app_state = state(
            LanguageId::English,
            RadarStatus::Disabled,
            crate::radar::RadarCache::default(),
        );
        app_state.codex_radar_enabled = false;
        app_state.last_poll_ok = true;
        app_state.data = Some(crate::core::models::UsageData {
            session: Some(crate::core::models::UsageSection {
                percentage: 42.0,
                resets_at: Some(std::time::UNIX_EPOCH + Duration::from_secs(1_785_900_000)),
            }),
            weekly: Some(crate::core::models::UsageSection {
                percentage: 98.0,
                resets_at: Some(std::time::UNIX_EPOCH + Duration::from_secs(1_785_917_790)),
            }),
            ..Default::default()
        });

        let text = format_widget_tooltip(&app_state, 0);

        assert!(widget_tooltip_available(&app_state));
        assert!(text.starts_with("5h → "));
        assert!(!text.contains("7d → "));
        assert!(!text.contains("CodexRadar"));
    }

    #[test]
    fn does_not_show_tooltip_for_weekly_reset_time_alone() {
        let mut app_state = state(
            LanguageId::English,
            RadarStatus::Disabled,
            crate::radar::RadarCache::default(),
        );
        app_state.codex_radar_enabled = false;
        app_state.last_poll_ok = true;
        app_state.show_5hour_window = false;
        app_state.data = Some(crate::core::models::UsageData {
            weekly: Some(crate::core::models::UsageSection {
                percentage: 98.0,
                resets_at: Some(std::time::UNIX_EPOCH + Duration::from_secs(1_785_917_790)),
            }),
            ..Default::default()
        });

        assert!(!widget_tooltip_available(&app_state));
        assert!(format_widget_tooltip(&app_state, 0).is_empty());
    }

    #[test]
    fn formats_complete_simplified_chinese_tooltip() {
        let app_state = state(
            LanguageId::SimplifiedChinese,
            RadarStatus::Ready,
            full_cache(),
        );

        let text = format_radar_tooltip(&app_state, 100 + 12 * 60);

        assert!(text.starts_with("CodexRadar · 12分前更新\r\n非个性化社区推荐\r\n"));
        assert!(text.contains("◆ 速度位\r\n  【Sol medium】 · IQ 91.1 · $3.74"));
        assert!(text.contains("◆ 聪明位\r\n  【GPT-5.5 xhigh】 · IQ 100.5 · $5.74"));
        assert!(text.contains("◆ 日常推荐\r\n  【Terra max】 · IQ 93.8 · $4.70 · 20.0分"));
        assert!(text.contains("◆ 偏智力\r\n  【Sol xhigh】 · IQ 105.8 · $6.19 · 30.0分"));
    }

    #[test]
    fn formats_untagged_radar_recommendations_as_community_daily() {
        let mut cache = full_cache();
        cache.radar.as_mut().unwrap().value = crate::radar::RadarRecommendations {
            speed: None,
            smart: None,
            daily_development: vec![
                recommendation("gpt-5.5", "high", 95.09, 3.663656),
                recommendation("gpt-5.6-luna", "max", 95.09, 0.47011),
            ],
        };
        let app_state = state(LanguageId::English, RadarStatus::Ready, cache);

        let text = format_radar_tooltip(&app_state, 100 + 60);

        assert!(text.contains(
            "◆ Community daily\r\n  【GPT-5.5 high】 · IQ 95.1 · $3.66\r\n  【Luna max】 · IQ 95.1 · $0.47"
        ));
        assert!(!text.contains("◆ Speed"));
        assert!(!text.contains("◆ Smart"));
    }

    #[test]
    fn formats_partial_cached_english_tooltip() {
        let mut cache = full_cache();
        cache.computed = None;
        cache.last_refresh_complete = false;
        let app_state = state(LanguageId::English, RadarStatus::Ready, cache);

        let text = format_radar_tooltip(&app_state, 100 + 60);

        assert!(text.starts_with(
            "CodexRadar · Updated 1m ago\r\nNon-personalized community recommendation · Cached data · May be outdated\r\n"
        ));
        assert!(text.contains("◆ Daily\r\n  Data temporarily unavailable"));
        assert!(text.contains("◆ IQ-first\r\n  Data temporarily unavailable"));
    }

    #[test]
    fn formats_missing_radar_slot_as_unavailable() {
        let mut cache = full_cache();
        cache.radar.as_mut().unwrap().value.smart = None;
        let app_state = state(LanguageId::English, RadarStatus::Ready, cache);

        let text = format_radar_tooltip(&app_state, 100 + 60);

        assert!(text.contains("◆ Speed\r\n  【Sol medium】"));
        assert!(text.contains("◆ Smart\r\n  Data temporarily unavailable"));
    }

    #[test]
    fn marks_fully_restored_data_as_cached() {
        let mut app_state = state(LanguageId::English, RadarStatus::Ready, full_cache());
        app_state.radar.displaying_cached_data = true;

        let text = format_radar_tooltip(&app_state, 100 + 60);

        assert!(text.contains("Cached data · May be outdated"));
        assert!(text.contains("◆ Daily\r\n  【Terra max】"));
        assert!(text.contains("◆ IQ-first\r\n  【Sol xhigh】"));
    }

    #[test]
    fn formats_fresh_update_age_without_just_now_ago_grammar() {
        let app_state = state(
            LanguageId::SimplifiedChinese,
            RadarStatus::Ready,
            full_cache(),
        );

        let text = format_radar_tooltip(&app_state, 100);

        assert!(text.contains("1分前更新"));
        assert!(!text.contains("刚刚前更新"));
    }

    #[test]
    fn distinguishes_loading_and_fetch_errors() {
        let loading = state(
            LanguageId::English,
            RadarStatus::Loading,
            crate::radar::RadarCache::default(),
        );
        assert_eq!(
            format_radar_tooltip(&loading, 100),
            "Fetching CodexRadar recommendations..."
        );

        let failed = state(
            LanguageId::SimplifiedChinese,
            RadarStatus::Error,
            crate::radar::RadarCache::default(),
        );
        assert_eq!(format_radar_tooltip(&failed, 100), "雷达推荐数据获取异常");
    }

    #[test]
    fn uses_legacy_compatible_tool_info_size() {
        assert_eq!(
            tooltip_tool_info(HWND::default(), PWSTR::null()).cbSize,
            std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32
        );
        assert!(
            tooltip_tool_info(HWND::default(), PWSTR::null()).cbSize
                < std::mem::size_of::<TTTOOLINFOW>() as u32
        );
    }

    #[test]
    fn centers_tooltip_above_widget() {
        let monitor = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let widget = RECT {
            left: 1000,
            top: 1040,
            right: 1150,
            bottom: 1080,
        };

        assert_eq!(
            centered_tooltip_position(widget, 500, 100, monitor, 4),
            POINT { x: 825, y: 936 }
        );
    }

    #[test]
    fn clamps_centered_tooltip_and_falls_below_at_top_edge() {
        let monitor = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let right_edge_widget = RECT {
            left: 1850,
            top: 1040,
            right: 1920,
            bottom: 1080,
        };
        assert_eq!(
            centered_tooltip_position(right_edge_widget, 500, 100, monitor, 4),
            POINT { x: 1420, y: 936 }
        );

        let top_edge_widget = RECT {
            left: 500,
            top: 0,
            right: 650,
            bottom: 40,
        };
        assert_eq!(
            centered_tooltip_position(top_edge_widget, 300, 100, monitor, 4),
            POINT { x: 425, y: 44 }
        );
    }
}
