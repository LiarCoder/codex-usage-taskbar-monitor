use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, TOOLTIPS_CLASSW, TTDT_INITIAL,
    TTF_SUBCLASS, TTM_ACTIVATE, TTM_ADDTOOLW, TTM_NEWTOOLRECTW, TTM_POP, TTM_SETDELAYTIME,
    TTM_SETMAXTIPWIDTH, TTM_UPDATETIPTEXTW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
};

use super::*;
use crate::radar::{self as radar_data, CachedSource, Recommendation};

const RADAR_TOOLTIP_ID: usize = 1;
const RADAR_TOOLTIP_DELAY_MS: isize = 500;
const RADAR_TOOLTIP_MAX_WIDTH: i32 = 480;

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
                native::wide_str(&format_radar_tooltip(app_state, now_unix_secs()));
            (
                PWSTR(app_state.radar_tooltip_text.as_mut_ptr()),
                app_state.codex_radar_enabled,
            )
        };

        let mut tool = tooltip_tool_info(hwnd, text_pointer);
        let _ = SendMessageW(
            tooltip,
            TTM_ADDTOOLW,
            WPARAM(0),
            LPARAM(&mut tool as *mut _ as isize),
        );
        let _ = SendMessageW(
            tooltip,
            TTM_SETDELAYTIME,
            WPARAM(TTDT_INITIAL as usize),
            LPARAM(RADAR_TOOLTIP_DELAY_MS),
        );
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
        let enabled = app_state.codex_radar_enabled;
        let text = native::wide_str(&format_radar_tooltip(app_state, now));
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

pub(super) fn pop_radar_tooltip() {
    let tooltip = {
        let state = lock_state();
        state
            .as_ref()
            .and_then(|app_state| app_state.radar_tooltip_hwnd)
    };
    if let Some(tooltip) = tooltip {
        unsafe {
            let _ = SendMessageW(tooltip, TTM_POP, WPARAM(0), LPARAM(0));
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
        cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
        uFlags: TTF_SUBCLASS,
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
    let mut lines = vec![strings.radar_tooltip_header.replace("{age}", &age)];
    if !cache.last_refresh_complete {
        lines[0].push_str(" · ");
        lines[0].push_str(strings.radar_cached_warning);
    }

    lines.push(format_source_line(
        strings.radar_recommendation,
        cache.radar.as_ref(),
        strings.radar_data_unavailable,
    ));

    if let Some(computed) = cache.computed.as_ref() {
        lines.push(format_recommendation_line(
            strings.radar_iq_per_dollar,
            computed.value.iq_per_dollar.as_ref(),
            strings.radar_no_eligible,
        ));
        lines.push(format_recommendation_line(
            strings.radar_intelligence_weighted,
            computed.value.intelligence_weighted.as_ref(),
            strings.radar_no_eligible,
        ));
    } else {
        lines.push(format!(
            "{}  {}",
            strings.radar_iq_per_dollar, strings.radar_data_unavailable
        ));
        lines.push(format!(
            "{}  {}",
            strings.radar_intelligence_weighted, strings.radar_data_unavailable
        ));
    }

    lines.join("\r\n")
}

fn format_source_line(
    label: &str,
    source: Option<&CachedSource<Recommendation>>,
    unavailable: &str,
) -> String {
    format_recommendation_line(label, source.map(|source| &source.value), unavailable)
}

fn format_recommendation_line(
    label: &str,
    recommendation: Option<&Recommendation>,
    unavailable: &str,
) -> String {
    let Some(recommendation) = recommendation else {
        return format!("{label}  {unavailable}");
    };
    format!(
        "{}  {} {} · IQ {:.1} · ${:.2}",
        label,
        radar_data::model_display_name(&recommendation.model),
        recommendation.effort,
        recommendation.iq,
        recommendation.average_cost_usd
    )
}

fn format_radar_age(elapsed_secs: u64, strings: Strings) -> String {
    if elapsed_secs < 60 {
        strings.radar_just_now.to_string()
    } else if elapsed_secs < 60 * 60 {
        format!("{}{}", elapsed_secs / 60, strings.minute_suffix)
    } else {
        format!("{}{}", elapsed_secs / (60 * 60), strings.hour_suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recommendation(model: &str, effort: &str, iq: f64, cost: f64) -> Recommendation {
        Recommendation {
            model: model.to_string(),
            effort: effort.to_string(),
            iq,
            average_cost_usd: cost,
            valid_tasks: 112,
        }
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
            widget_visible: true,
            compact_mode: false,
            show_5hour_window: true,
            show_7day_window: true,
            codex_radar_enabled: true,
            codex_radar_consent_version: 1,
            radar: RadarRuntimeState {
                status,
                cache,
                in_flight: false,
                request_generation: 0,
            },
            radar_tooltip_hwnd: None,
            radar_tooltip_text: vec![0],
        }
    }

    fn full_cache() -> crate::radar::RadarCache {
        let mut cache = crate::radar::RadarCache::default();
        cache.radar = Some(CachedSource {
            value: recommendation("gpt-5.6-sol", "high", 89.73, 4.998),
            validated_at_unix: 100,
            validator: None,
        });
        cache.computed = Some(CachedSource {
            value: crate::radar::ComputedRecommendations {
                iq_per_dollar: Some(recommendation("gpt-5.6-terra", "max", 93.75, 4.70)),
                intelligence_weighted: Some(recommendation("gpt-5.6-sol", "xhigh", 105.8, 6.19)),
            },
            validated_at_unix: 100,
            validator: None,
        });
        cache.last_refresh_complete = true;
        cache
    }

    #[test]
    fn formats_complete_simplified_chinese_tooltip() {
        let app_state = state(
            LanguageId::SimplifiedChinese,
            RadarStatus::Ready,
            full_cache(),
        );

        let text = format_radar_tooltip(&app_state, 100 + 12 * 60);

        assert!(text.contains("12分前更新"));
        assert!(text.contains("雷达推荐  Sol high · IQ 89.7 · $5.00"));
        assert!(text.contains("IQ/$  Terra max · IQ 93.8 · $4.70"));
        assert!(text.contains("偏智力  Sol xhigh · IQ 105.8 · $6.19"));
    }

    #[test]
    fn formats_partial_cached_english_tooltip() {
        let mut cache = full_cache();
        cache.computed = None;
        cache.last_refresh_complete = false;
        let app_state = state(LanguageId::English, RadarStatus::Ready, cache);

        let text = format_radar_tooltip(&app_state, 100 + 60);

        assert!(text.contains("Cached data · May be outdated"));
        assert!(text.contains("IQ/$  Data temporarily unavailable"));
        assert!(text.contains("IQ-first  Data temporarily unavailable"));
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
}
