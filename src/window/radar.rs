use super::*;
use crate::radar as radar_data;

pub(super) fn initialize_radar(hwnd: HWND) {
    let (enabled, delay) = {
        let state = lock_state();
        let Some(app_state) = state.as_ref() else {
            return;
        };
        (
            app_state.codex_radar_enabled,
            radar_data::next_due_in_secs(&app_state.radar.cache, now_unix_secs()),
        )
    };

    if !enabled {
        return;
    }
    if delay == 0 {
        let _ = begin_radar_refresh(hwnd, false);
    } else {
        set_radar_timer(hwnd, delay);
    }
}

pub(super) fn begin_radar_refresh(hwnd: HWND, manual: bool) -> bool {
    let now = now_unix_secs();
    let (generation, validators, cache_before_request) = {
        let mut state = lock_state();
        let Some(app_state) = state.as_mut() else {
            return false;
        };
        if !app_state.codex_radar_enabled || app_state.radar.in_flight {
            return false;
        }
        if manual
            && app_state.radar.cache.last_attempt_unix.is_some_and(|last| {
                now.saturating_sub(last) < radar_data::MANUAL_REFRESH_COOLDOWN_SECS
            })
        {
            return false;
        }

        app_state.radar.in_flight = true;
        app_state.radar.request_generation = app_state.radar.request_generation.wrapping_add(1);
        app_state.radar.cache.last_attempt_unix = Some(now);
        if !app_state.radar.cache.has_any_data() {
            app_state.radar.status = RadarStatus::Loading;
        }

        (
            app_state.radar.request_generation,
            app_state.radar.cache.validators(),
            app_state.radar.cache.clone(),
        )
    };

    radar_data::save_cache(&cache_before_request);
    refresh_radar_tooltip_text(hwnd);
    unsafe {
        let _ = KillTimer(hwnd, TIMER_RADAR);
    }

    let send_hwnd = SendHwnd::from_hwnd(hwnd);
    std::thread::spawn(move || {
        let refresh = radar_data::fetch_recommendations(&validators);
        if let Some(error) = refresh.radar.error() {
            diagnose::log(format!("CodexRadar insights unavailable: {error}"));
        }
        if let Some(error) = refresh.computed.error() {
            diagnose::log(format!("CodexRadar efficiency data unavailable: {error}"));
        }

        let completed_at = now_unix_secs();
        let cache_to_save = {
            let mut state = lock_state();
            let Some(app_state) = state.as_mut() else {
                return;
            };
            if !app_state.codex_radar_enabled || app_state.radar.request_generation != generation {
                return;
            }

            let complete =
                radar_data::apply_refresh(&mut app_state.radar.cache, refresh, completed_at);
            app_state.radar.in_flight = false;
            app_state.radar.status = if app_state.radar.cache.has_any_data() {
                RadarStatus::Ready
            } else {
                RadarStatus::Error
            };
            let radar_model = app_state
                .radar
                .cache
                .radar
                .as_ref()
                .map(|source| radar_data::model_display_name(&source.value.model))
                .unwrap_or_else(|| "unavailable".to_string());
            diagnose::log(format!(
                "CodexRadar refresh finished: complete={complete} radar={radar_model} computed={}",
                app_state.radar.cache.computed.is_some()
            ));
            app_state.radar.cache.clone()
        };

        radar_data::save_cache(&cache_to_save);
        unsafe {
            let _ = PostMessageW(
                send_hwnd.to_hwnd(),
                WM_APP_RADAR_UPDATED,
                WPARAM(0),
                LPARAM(0),
            );
        }
    });

    true
}

pub(super) fn handle_radar_updated(hwnd: HWND) -> LRESULT {
    sync_radar_tooltip(hwnd);
    let delay = {
        let state = lock_state();
        state
            .as_ref()
            .map(|app_state| radar_data::next_due_in_secs(&app_state.radar.cache, now_unix_secs()))
    };
    if let Some(delay) = delay {
        set_radar_timer(hwnd, delay.max(1));
    }
    LRESULT(0)
}

fn set_radar_timer(hwnd: HWND, delay_secs: u64) {
    let delay_ms = delay_secs.saturating_mul(1000).min(u32::MAX as u64) as u32;
    unsafe {
        SetTimer(hwnd, TIMER_RADAR, delay_ms.max(1000), None);
    }
}
