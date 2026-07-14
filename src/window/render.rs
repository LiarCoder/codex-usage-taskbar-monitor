//! GDI rendering helpers for widget content.

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, UpdateLayeredWindow, ULW_ALPHA};

use crate::localization::Strings;
use crate::platform::native::{self, Color};

use super::layout::{
    refresh_dpi, sc, total_widget_width, BAR_RIGHT_MARGIN, CORNER_RADIUS, DIVIDER_RIGHT_MARGIN,
    LABEL_RIGHT_MARGIN, LABEL_WIDTH, LEFT_DIVIDER_W, RIGHT_MARGIN, SEGMENT_COUNT, SEGMENT_GAP,
    SEGMENT_H, SEGMENT_W, TEXT_WIDTH, WIDGET_HEIGHT,
};
use super::state::lock_state;

/// Bundles the immutable drawing parameters shared across the
/// GDI rendering helpers so each function stays under clippy's
/// 7-argument threshold.
pub(crate) struct RenderContext {
    pub(crate) hdc: HDC,
    pub(crate) is_dark: bool,
    pub(crate) text_color: Color,
    pub(crate) accent: Color,
    pub(crate) track: Color,
    pub(crate) compact_mode: bool,
}

pub(crate) fn accent_color(is_dark: bool) -> Color {
    if is_dark {
        Color::from_hex("#F5F5F5")
    } else {
        Color::from_hex("#1F1F1F")
    }
}

/// Paint all widget content onto a DC with a given background color.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_content(
    ctx: &RenderContext,
    width: i32,
    height: i32,
    bg: &Color,
    strings: Strings,
    session_pct: f64,
    session_text: &str,
    weekly_pct: f64,
    weekly_text: &str,
    show_session: bool,
    show_weekly: bool,
) {
    unsafe {
        let client_rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };

        let bg_brush = CreateSolidBrush(COLORREF(bg.to_colorref()));
        FillRect(ctx.hdc, &client_rect, bg_brush);
        let _ = DeleteObject(bg_brush);

        // Left divider
        let divider_h = sc(25);
        let divider_top = (height - divider_h) / 2;
        let divider_bottom = divider_top + divider_h;

        let (div_left, div_right) = if ctx.is_dark {
            ((80, 80, 80), (40, 40, 40))
        } else {
            ((160, 160, 160), (230, 230, 230))
        };

        let left_brush = CreateSolidBrush(COLORREF(native::colorref(
            div_left.0, div_left.1, div_left.2,
        )));
        let left_rect = RECT {
            left: 0,
            top: divider_top,
            right: sc(2),
            bottom: divider_bottom,
        };
        FillRect(ctx.hdc, &left_rect, left_brush);
        let _ = DeleteObject(left_brush);

        let right_brush = CreateSolidBrush(COLORREF(native::colorref(
            div_right.0,
            div_right.1,
            div_right.2,
        )));
        let right_rect = RECT {
            left: sc(2),
            top: divider_top,
            right: sc(3),
            bottom: divider_bottom,
        };
        FillRect(ctx.hdc, &right_rect, right_brush);
        let _ = DeleteObject(right_brush);

        let content_x = sc(LEFT_DIVIDER_W) + sc(DIVIDER_RIGHT_MARGIN);
        let row2_y = height - sc(5) - sc(SEGMENT_H);
        let row1_y = row2_y - sc(10) - sc(SEGMENT_H);

        let _ = SetBkMode(ctx.hdc, TRANSPARENT);
        let _ = SetTextColor(ctx.hdc, COLORREF(ctx.text_color.to_colorref()));

        let font_name = native::wide_str("Segoe UI");
        let font = CreateFontW(
            sc(-12),
            0,
            0,
            0,
            FW_MEDIUM.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_TT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR::from_raw(font_name.as_ptr()),
        );
        let old_font = SelectObject(ctx.hdc, font);

        match (show_session, show_weekly) {
            (true, true) => {
                draw_row(
                    ctx,
                    content_x,
                    row1_y,
                    strings.session_window,
                    session_pct,
                    session_text,
                );
                draw_row(
                    ctx,
                    content_x,
                    row2_y,
                    strings.weekly_window,
                    weekly_pct,
                    weekly_text,
                );
            }
            (true, false) => draw_row(
                ctx,
                content_x,
                (height - sc(SEGMENT_H)) / 2,
                strings.session_window,
                session_pct,
                session_text,
            ),
            (false, true) => draw_row(
                ctx,
                content_x,
                (height - sc(SEGMENT_H)) / 2,
                strings.weekly_window,
                weekly_pct,
                weekly_text,
            ),
            (false, false) => draw_centered_text(ctx, width, height, strings.no_data),
        }

        SelectObject(ctx.hdc, old_font);
        let _ = DeleteObject(font);
    }
}

pub(crate) fn draw_centered_text(ctx: &RenderContext, width: i32, height: i32, text: &str) {
    unsafe {
        let mut text_wide: Vec<u16> = text.encode_utf16().collect();
        let mut rect = RECT {
            left: sc(LEFT_DIVIDER_W) + sc(DIVIDER_RIGHT_MARGIN),
            top: 0,
            right: width - sc(RIGHT_MARGIN),
            bottom: height,
        };
        let _ = DrawTextW(
            ctx.hdc,
            &mut text_wide,
            &mut rect,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

pub(crate) fn draw_row(
    ctx: &RenderContext,
    x: i32,
    y: i32,
    label: &str,
    percent: f64,
    bar_text: &str,
) {
    let seg_h = sc(SEGMENT_H);
    let segment_count = SEGMENT_COUNT;
    // codex-only: always use the generic text color
    let value_color = ctx.text_color;

    unsafe {
        let _ = SetTextColor(ctx.hdc, COLORREF(ctx.text_color.to_colorref()));
        let mut label_wide: Vec<u16> = label.encode_utf16().collect();
        let mut label_rect = RECT {
            left: x,
            top: y,
            right: x + sc(LABEL_WIDTH),
            bottom: y + seg_h,
        };
        let _ = DrawTextW(
            ctx.hdc,
            &mut label_wide,
            &mut label_rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );

        let model_x = x + sc(LABEL_WIDTH) + sc(LABEL_RIGHT_MARGIN);
        draw_usage_bar(
            ctx,
            model_x,
            y,
            segment_count,
            percent,
            bar_text,
            &value_color,
        );
    }
}

pub(crate) fn draw_usage_bar(
    ctx: &RenderContext,
    bar_x: i32,
    y: i32,
    segment_count: i32,
    percent: f64,
    text: &str,
    text_color: &Color,
) {
    let seg_w = sc(SEGMENT_W);
    let seg_h = sc(SEGMENT_H);
    let seg_gap = sc(SEGMENT_GAP);
    let corner_r = sc(CORNER_RADIUS);

    unsafe {
        if !ctx.compact_mode {
            let percent_clamped = percent.clamp(0.0, 100.0);
            let segment_percent = 100.0 / segment_count as f64;

            for i in 0..segment_count {
                let seg_x = bar_x + i * (seg_w + seg_gap);
                let seg_start = (i as f64) * segment_percent;
                let seg_end = seg_start + segment_percent;

                let seg_rect = RECT {
                    left: seg_x,
                    top: y,
                    right: seg_x + seg_w,
                    bottom: y + seg_h,
                };

                if percent_clamped >= seg_end {
                    draw_rounded_rect(ctx.hdc, &seg_rect, &ctx.accent, corner_r);
                } else if percent_clamped <= seg_start {
                    draw_rounded_rect(ctx.hdc, &seg_rect, &ctx.track, corner_r);
                } else {
                    draw_rounded_rect(ctx.hdc, &seg_rect, &ctx.track, corner_r);
                    let fraction = (percent_clamped - seg_start) / segment_percent;
                    let fill_width = (seg_w as f64 * fraction) as i32;
                    if fill_width > 0 {
                        let fill_rect = RECT {
                            left: seg_x,
                            top: y,
                            right: seg_x + fill_width,
                            bottom: y + seg_h,
                        };
                        let rgn = CreateRoundRectRgn(
                            seg_rect.left,
                            seg_rect.top,
                            seg_rect.right + 1,
                            seg_rect.bottom + 1,
                            corner_r * 2,
                            corner_r * 2,
                        );
                        let _ = SelectClipRgn(ctx.hdc, rgn);
                        let brush = CreateSolidBrush(COLORREF(ctx.accent.to_colorref()));
                        FillRect(ctx.hdc, &fill_rect, brush);
                        let _ = DeleteObject(brush);
                        let _ = SelectClipRgn(ctx.hdc, HRGN::default());
                        let _ = DeleteObject(rgn);
                    }
                }
            }
        }

        let text_x = if ctx.compact_mode {
            bar_x
        } else {
            bar_x + segment_count * (seg_w + seg_gap) - seg_gap + sc(BAR_RIGHT_MARGIN)
        };
        let mut text_wide: Vec<u16> = text.encode_utf16().collect();
        let mut text_rect = RECT {
            left: text_x,
            top: y,
            right: text_x + sc(TEXT_WIDTH),
            bottom: y + seg_h,
        };
        let _ = SetTextColor(ctx.hdc, COLORREF(text_color.to_colorref()));
        let _ = DrawTextW(
            ctx.hdc,
            &mut text_wide,
            &mut text_rect,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }
}

pub(crate) fn draw_rounded_rect(hdc: HDC, rect: &RECT, color: &Color, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(COLORREF(color.to_colorref()));
        let rgn = CreateRoundRectRgn(
            rect.left,
            rect.top,
            rect.right + 1,
            rect.bottom + 1,
            radius * 2,
            radius * 2,
        );
        let _ = FillRgn(hdc, rgn, brush);
        let _ = DeleteObject(rgn);
        let _ = DeleteObject(brush);
    }
}

/// Render widget content and push to the layered window via UpdateLayeredWindow.
/// Renders fully opaque with the actual taskbar background colour so that
/// ClearType sub-pixel font rendering can be used for crisp, OS-native text.
pub(super) fn render_layered() {
    refresh_dpi();
    let (
        hwnd_val,
        is_dark,
        embedded,
        strings,
        codex_session_pct,
        codex_session_text,
        codex_weekly_pct,
        codex_weekly_text,
        show_session,
        show_weekly,
        compact_mode,
    ) = {
        let state = lock_state();
        match state.as_ref() {
            Some(s) => (
                s.hwnd,
                s.is_dark,
                s.embedded,
                s.language.strings(),
                s.display_percentage(s.session_percent, s.session_available),
                s.session_text.clone(),
                s.display_percentage(s.weekly_percent, s.weekly_available),
                s.weekly_text.clone(),
                s.show_5hour_window && s.session_available,
                s.show_7day_window && s.weekly_available,
                s.compact_mode,
            ),
            None => return,
        }
    };

    let hwnd = hwnd_val.to_hwnd();

    // For non-embedded fallback, just invalidate and let WM_PAINT handle it
    if !embedded {
        unsafe {
            let _ = InvalidateRect(hwnd, None, false);
        }
        return;
    }

    let width = total_widget_width();
    let height = sc(WIDGET_HEIGHT);

    let codex_accent = accent_color(is_dark);
    let track = if is_dark {
        Color::from_hex("#444444")
    } else {
        Color::from_hex("#AAAAAA")
    };
    let text_color = if is_dark {
        Color::from_hex("#888888")
    } else {
        Color::from_hex("#404040")
    };
    let bg_color = if is_dark {
        Color::from_hex("#1C1C1C")
    } else {
        Color::from_hex("#F3F3F3")
    };

    unsafe {
        let screen_dc = GetDC(hwnd);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let mem_dc = CreateCompatibleDC(screen_dc);
        let dib =
            CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();

        if dib.is_invalid() || bits.is_null() {
            let _ = DeleteDC(mem_dc);
            ReleaseDC(hwnd, screen_dc);
            return;
        }

        let old_bmp = SelectObject(mem_dc, dib);
        let pixel_count = (width * height) as usize;

        // Render once with the actual taskbar background colour.
        // Using an opaque background lets us use CLEARTYPE_QUALITY for
        // sub-pixel font rendering that matches the rest of the OS.
        let ctx = RenderContext {
            hdc: mem_dc,
            is_dark,
            text_color,
            accent: codex_accent,
            track,
            compact_mode,
        };
        paint_content(
            &ctx,
            width,
            height,
            &bg_color,
            strings,
            codex_session_pct,
            &codex_session_text,
            codex_weekly_pct,
            &codex_weekly_text,
            show_session,
            show_weekly,
        );

        // Background pixels → alpha 1 (nearly invisible but still hittable for right-click).
        // Content pixels → fully opaque (preserves ClearType sub-pixel rendering).
        let bg_bgr = bg_color.to_colorref();
        let pixel_data = std::slice::from_raw_parts_mut(bits as *mut u32, pixel_count);
        for px in pixel_data.iter_mut() {
            let rgb = *px & 0x00FFFFFF;
            if rgb == bg_bgr {
                *px = 0x01000000;
            } else {
                *px = rgb | 0xFF000000;
            }
        }

        // Push to window via UpdateLayeredWindow
        let pt_src = POINT { x: 0, y: 0 };
        let sz = SIZE {
            cx: width,
            cy: height,
        };
        let blend = BLENDFUNCTION {
            BlendOp: 0, // AC_SRC_OVER
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: 1, // AC_SRC_ALPHA
        };

        let _ = UpdateLayeredWindow(
            hwnd,
            screen_dc,
            None,
            Some(&sz),
            mem_dc,
            Some(&pt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );

        // Cleanup
        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(dib);
        let _ = DeleteDC(mem_dc);
        ReleaseDC(hwnd, screen_dc);
    }
}

/// Paint for non-embedded fallback (normal WM_PAINT path)
pub(super) fn paint(hdc: HDC, hwnd: HWND) {
    let (
        is_dark,
        strings,
        codex_session_pct,
        codex_session_text,
        codex_weekly_pct,
        codex_weekly_text,
        show_session,
        show_weekly,
        compact_mode,
    ) = {
        let state = lock_state();
        match state.as_ref() {
            Some(s) => (
                s.is_dark,
                s.language.strings(),
                s.display_percentage(s.session_percent, s.session_available),
                s.session_text.clone(),
                s.display_percentage(s.weekly_percent, s.weekly_available),
                s.weekly_text.clone(),
                s.show_5hour_window && s.session_available,
                s.show_7day_window && s.weekly_available,
                s.compact_mode,
            ),
            None => return,
        }
    };

    let codex_accent = accent_color(is_dark);
    let track = if is_dark {
        Color::from_hex("#444444")
    } else {
        Color::from_hex("#AAAAAA")
    };
    let text_color = if is_dark {
        Color::from_hex("#888888")
    } else {
        Color::from_hex("#404040")
    };
    let bg_color = if is_dark {
        Color::from_hex("#1C1C1C")
    } else {
        Color::from_hex("#F3F3F3")
    };

    unsafe {
        let mut client_rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut client_rect);
        let width = client_rect.right - client_rect.left;
        let height = client_rect.bottom - client_rect.top;

        if width <= 0 || height <= 0 {
            return;
        }

        let mem_dc = CreateCompatibleDC(hdc);
        let mem_bmp = CreateCompatibleBitmap(hdc, width, height);
        let old_bmp = SelectObject(mem_dc, mem_bmp);

        let ctx = RenderContext {
            hdc: mem_dc,
            is_dark,
            text_color,
            accent: codex_accent,
            track,
            compact_mode,
        };
        paint_content(
            &ctx,
            width,
            height,
            &bg_color,
            strings,
            codex_session_pct,
            &codex_session_text,
            codex_weekly_pct,
            &codex_weekly_text,
            show_session,
            show_weekly,
        );

        let _ = BitBlt(hdc, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);

        SelectObject(mem_dc, old_bmp);
        let _ = DeleteObject(mem_bmp);
        let _ = DeleteDC(mem_dc);
    }
}
