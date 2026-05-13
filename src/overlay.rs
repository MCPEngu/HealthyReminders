use std::{sync::mpsc::Sender, thread};

use anyhow::{anyhow, Context, Result};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
            GetStockObject, SelectObject, SetBkMode, SetTextColor, CLEARTYPE_QUALITY,
            CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH, DT_CENTER,
            DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HDC, OUT_DEFAULT_PRECIS,
            PAINTSTRUCT, TRANSPARENT,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
            GetMessageW, GetSystemMetrics, GetWindowLongPtrW, KillTimer, LoadCursorW,
            PostQuitMessage, RegisterClassW, SendMessageW, SetForegroundWindow, SetTimer,
            SetWindowLongPtrW, SetWindowTextW, ShowWindow, TranslateMessage, CREATESTRUCTW,
            GWLP_USERDATA, HMENU, IDC_ARROW, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WM_COMMAND,
            WM_DESTROY, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETFONT, WM_TIMER, WNDCLASSW,
            WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            WS_TABSTOP, WS_VISIBLE,
        },
    },
};

use crate::{
    core::{self, AppConfig, OverlayAccent, OverlayStyle},
    scheduler::{ReminderKind, SchedulerCommand},
};

const CLASS_NAME: &str = "HealthyRemindersOverlayWindow";
const ID_ACTION: i32 = 901;
const ID_SKIP: i32 = 902;
const TIMER_ID: usize = 1;

struct OverlayState {
    kind: ReminderKind,
    config: AppConfig,
    scheduler_tx: Sender<SchedulerCommand>,
    remaining: u64,
    action_button: isize,
    skip_button: isize,
}

pub fn spawn_overlay(
    kind: ReminderKind,
    config: AppConfig,
    scheduler_tx: Sender<SchedulerCommand>,
) -> Result<()> {
    thread::Builder::new()
        .name(format!("ReminderOverlay-{kind:?}"))
        .spawn(move || {
            if let Err(error) = run_overlay(kind, config, scheduler_tx) {
                log::warn!("overlay failed: {error:#}");
            }
        })
        .context("failed to spawn reminder overlay")?;
    Ok(())
}

fn run_overlay(
    kind: ReminderKind,
    config: AppConfig,
    scheduler_tx: Sender<SchedulerCommand>,
) -> Result<()> {
    register_class()?;
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(800);
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(600);
    let remaining = countdown_seconds(kind, &config);

    let state = Box::new(OverlayState {
        kind,
        config,
        scheduler_tx,
        remaining,
        action_button: 0,
        skip_button: 0,
    });
    let state_ptr = Box::into_raw(state);

    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let hinstance = HINSTANCE(module.0);
    let class_name = core::wide_null(CLASS_NAME);
    let title = core::wide_null(core::APP_NAME);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            width,
            height,
            HWND(0),
            HMENU(0),
            hinstance,
            Some(state_ptr as *const _),
        )
    };

    if hwnd.0 == 0 {
        unsafe {
            drop(Box::from_raw(state_ptr));
        }
        return Err(anyhow!("CreateWindowExW overlay returned null HWND"));
    }

    create_buttons(hwnd, width, height)?;
    unsafe {
        let _ = SetTimer(hwnd, TIMER_ID, 1000, None);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }

    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, HWND(0), 0, 0) }.0 > 0 {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    Ok(())
}

fn register_class() -> Result<()> {
    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let hinstance = HINSTANCE(module.0);
    let class_name = core::wide_null(CLASS_NAME);
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.context("LoadCursorW failed")?;
    let wc = WNDCLASSW {
        lpfnWndProc: Some(overlay_wnd_proc),
        hInstance: hinstance,
        hCursor: cursor,
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };

    let atom = unsafe { RegisterClassW(&wc) };
    if atom == 0 {
        log::debug!("overlay class is already registered or RegisterClassW failed");
    }
    Ok(())
}

fn create_buttons(parent: HWND, width: i32, height: i32) -> Result<()> {
    let action = create_button(
        parent,
        ID_ACTION,
        "",
        (width / 2) - 230,
        height - 150,
        210,
        48,
    )?;
    let skip = create_button(parent, ID_SKIP, "", (width / 2) + 20, height - 150, 210, 48)?;

    with_state_mut(parent, |state| {
        state.action_button = action.0;
        state.skip_button = skip.0;
        update_button_text(state);
    });
    Ok(())
}

fn create_button(
    parent: HWND,
    id: i32,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Result<HWND> {
    let class_name = core::wide_null("BUTTON");
    let text = core::wide_null(text);
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            x,
            y,
            width,
            height,
            parent,
            HMENU(id as isize),
            HINSTANCE(0),
            None,
        )
    };
    if hwnd.0 == 0 {
        return Err(anyhow!("CreateWindowExW overlay button failed"));
    }

    let font = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    if font.0 != 0 {
        unsafe {
            let _ = SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
    }
    Ok(hwnd)
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match std::panic::catch_unwind(|| overlay_wnd_proc_inner(hwnd, msg, wparam, lparam)) {
        Ok(result) => result,
        Err(_) => {
            log::error!("overlay window procedure panicked");
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

fn overlay_wnd_proc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            if !create.is_null() {
                let state = unsafe { (*create).lpCreateParams as *mut OverlayState };
                unsafe {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                }
            }
            return LRESULT(1);
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as i32;
            match id {
                ID_ACTION => {
                    with_state_mut(hwnd, |state| {
                        if state.remaining == 0 {
                            let _ = state
                                .scheduler_tx
                                .send(SchedulerCommand::ToastActivated(state.kind));
                            unsafe {
                                let _ = DestroyWindow(hwnd);
                            }
                        }
                    });
                    return LRESULT(0);
                }
                ID_SKIP => {
                    with_state_mut(hwnd, |state| {
                        if state.remaining == 0 {
                            if state.kind == ReminderKind::Stand {
                                let snooze = state.config.movement_snooze_minutes.clamp(5, 15);
                                let _ = state
                                    .scheduler_tx
                                    .send(SchedulerCommand::Snooze(ReminderKind::Stand, snooze));
                            }
                            unsafe {
                                let _ = DestroyWindow(hwnd);
                            }
                        }
                    });
                    return LRESULT(0);
                }
                _ => {}
            }
        }
        WM_TIMER => {
            with_state_mut(hwnd, |state| {
                if state.remaining > 0 {
                    state.remaining -= 1;
                    update_button_text(state);
                    unsafe {
                        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(hwnd, None, BOOL(1));
                    }
                }
            });
            return LRESULT(0);
        }
        WM_PAINT => {
            paint(hwnd);
            return LRESULT(0);
        }
        WM_DESTROY => {
            unsafe {
                let _ = KillTimer(hwnd, TIMER_ID);
                PostQuitMessage(0);
            }
            return LRESULT(0);
        }
        WM_NCDESTROY => {
            let state = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
            if state != 0 {
                unsafe {
                    drop(Box::from_raw(state as *mut OverlayState));
                }
            }
        }
        _ => {}
    }

    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    if hdc.0 == 0 {
        return;
    }

    let mut rect = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_ok() {
        with_state_mut(hwnd, |state| paint_state(hdc, rect, state));
    }

    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}

fn paint_state(hdc: HDC, rect: RECT, state: &mut OverlayState) {
    let palette = palette(state.config.overlay_style, state.config.overlay_accent);
    let background = unsafe { CreateSolidBrush(palette.background) };
    unsafe {
        let _ = FillRect(hdc, &rect, background);
        let _ = DeleteObject(background);
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, palette.text);
    }

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    let accent_rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: accent_height(state.config.overlay_style),
    };
    let accent = unsafe { CreateSolidBrush(palette.accent) };
    unsafe {
        let _ = FillRect(hdc, &accent_rect, accent);
        let _ = DeleteObject(accent);
    }

    draw_text(
        hdc,
        title_font_height(state.config.overlay_style),
        700,
        title(state.kind, state.config.language),
        RECT {
            left: 80,
            top: height / 2 - 190,
            right: width - 80,
            bottom: height / 2 - 95,
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        palette.text,
    );

    let message = reminder_message(state);
    draw_text(
        hdc,
        26,
        400,
        &message,
        RECT {
            left: 160,
            top: height / 2 - 70,
            right: width - 160,
            bottom: height / 2 + 20,
        },
        DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
        palette.subtle,
    );

    let countdown = countdown_text(state);
    draw_text(
        hdc,
        24,
        600,
        &countdown,
        RECT {
            left: 160,
            top: height / 2 + 52,
            right: width - 160,
            bottom: height / 2 + 105,
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        palette.accent,
    );
}

fn draw_text(
    hdc: HDC,
    height: i32,
    weight: i32,
    text: &str,
    mut rect: RECT,
    format: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT,
    color: COLORREF,
) {
    let face = core::wide_null("Segoe UI");
    let font = unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            DEFAULT_PITCH.0 as u32,
            PCWSTR(face.as_ptr()),
        )
    };
    let mut text = core::wide_null(text);
    unsafe {
        let old_font = SelectObject(hdc, font);
        let _ = SetTextColor(hdc, color);
        let _ = DrawTextW(hdc, &mut text, &mut rect, format);
        let _ = SelectObject(hdc, old_font);
        let _ = DeleteObject(font);
    }
}

fn with_state_mut(hwnd: HWND, f: impl FnOnce(&mut OverlayState)) {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut OverlayState;
    if !ptr.is_null() {
        f(unsafe { &mut *ptr });
    }
}

fn update_button_text(state: &OverlayState) {
    let (action, skip) = if state.remaining > 0 {
        let label = wait_label(state.config.language, state.remaining);
        (label.clone(), label)
    } else {
        (
            action_label(state.kind, state.config.language).to_owned(),
            skip_label(state.kind, state.config.language).to_owned(),
        )
    };
    set_text(HWND(state.action_button), &action);
    set_text(HWND(state.skip_button), &skip);
}

fn set_text(hwnd: HWND, text: &str) {
    if hwnd.0 == 0 {
        return;
    }
    let text = core::wide_null(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(text.as_ptr()));
    }
}

fn countdown_text(state: &OverlayState) -> String {
    if state.remaining > 0 {
        match (state.config.language, state.kind) {
            (core::Language::Vietnamese, ReminderKind::Eyes) => {
                format!("20-20-20: nhìn xa, còn {} giây", state.remaining)
            }
            (core::Language::Vietnamese, _) => format!("Focus mode: còn {} giây", state.remaining),
            (core::Language::English, ReminderKind::Eyes) => {
                format!("20-20-20: look away, {} seconds left", state.remaining)
            }
            (core::Language::English, _) => {
                format!("Focus mode: {} seconds left", state.remaining)
            }
        }
    } else {
        match state.config.language {
            core::Language::Vietnamese => "Sẵn sàng để hoàn tất hoặc bỏ qua.".to_owned(),
            core::Language::English => "Ready to finish or skip.".to_owned(),
        }
    }
}

fn title(kind: ReminderKind, language: core::Language) -> &'static str {
    match (language, kind) {
        (core::Language::Vietnamese, ReminderKind::Water) => "Uống nước",
        (core::Language::Vietnamese, ReminderKind::Eyes) => "Nghỉ mắt",
        (core::Language::Vietnamese, ReminderKind::Stand) => "Đứng dậy",
        (core::Language::Vietnamese, ReminderKind::Work) => "Quay lại làm việc",
        (core::Language::English, ReminderKind::Water) => "Drink water",
        (core::Language::English, ReminderKind::Eyes) => "Eye rest",
        (core::Language::English, ReminderKind::Stand) => "Stand up",
        (core::Language::English, ReminderKind::Work) => "Back to work",
    }
}

fn message(kind: ReminderKind, language: core::Language) -> &'static str {
    match (language, kind) {
        (core::Language::Vietnamese, ReminderKind::Water) => {
            "Bổ sung nước và ghi nhận một ly khi bạn đã uống xong."
        }
        (core::Language::Vietnamese, ReminderKind::Eyes) => {
            "Nhìn xa và để mắt nghỉ đủ thời gian trước khi tiếp tục."
        }
        (core::Language::Vietnamese, ReminderKind::Stand) => "Rời ghế và làm một vận động ngắn.",
        (core::Language::Vietnamese, ReminderKind::Work) => {
            "Hết thời gian đứng dậy. Sẵn sàng quay lại công việc."
        }
        (core::Language::English, ReminderKind::Water) => {
            "Hydrate, then log one glass when you are done."
        }
        (core::Language::English, ReminderKind::Eyes) => {
            "Look away and rest your eyes before continuing."
        }
        (core::Language::English, ReminderKind::Stand) => {
            "Leave your chair and do a short movement break."
        }
        (core::Language::English, ReminderKind::Work) => {
            "Your movement break is over. Ready to get back to work."
        }
    }
}

fn reminder_message(state: &OverlayState) -> String {
    match state.kind {
        ReminderKind::Stand => core::movement_suggestion(&state.config).to_owned(),
        _ => message(state.kind, state.config.language).to_owned(),
    }
}

fn action_label(kind: ReminderKind, language: core::Language) -> &'static str {
    match (language, kind) {
        (core::Language::Vietnamese, ReminderKind::Water) => "Tôi đã uống",
        (core::Language::Vietnamese, ReminderKind::Eyes) => "Xong",
        (core::Language::Vietnamese, ReminderKind::Stand) => "Đã đứng dậy",
        (core::Language::Vietnamese, ReminderKind::Work) => "Bắt đầu làm việc",
        (core::Language::English, ReminderKind::Water) => "I drank",
        (core::Language::English, ReminderKind::Eyes) => "Done",
        (core::Language::English, ReminderKind::Stand) => "Done",
        (core::Language::English, ReminderKind::Work) => "Start working",
    }
}

fn skip_label(kind: ReminderKind, language: core::Language) -> &'static str {
    match (language, kind) {
        (core::Language::Vietnamese, ReminderKind::Stand) => "Đang họp",
        (core::Language::Vietnamese, _) => "Bỏ qua",
        (core::Language::English, ReminderKind::Stand) => "In a meeting",
        (core::Language::English, _) => "Skip",
    }
}

fn wait_label(language: core::Language, remaining: u64) -> String {
    match language {
        core::Language::Vietnamese => format!("Đợi {remaining}s"),
        core::Language::English => format!("Wait {remaining}s"),
    }
}

fn countdown_seconds(kind: ReminderKind, config: &AppConfig) -> u64 {
    match kind {
        ReminderKind::Eyes => config.eye_countdown_seconds.clamp(10, 60),
        ReminderKind::Water | ReminderKind::Stand => {
            if config.focus_mode_enabled {
                config.focus_countdown_seconds.clamp(5, 300)
            } else {
                0
            }
        }
        ReminderKind::Work => 0,
    }
}

struct Palette {
    background: COLORREF,
    text: COLORREF,
    subtle: COLORREF,
    accent: COLORREF,
}

fn palette(style: OverlayStyle, accent: OverlayAccent) -> Palette {
    let accent = accent_color(accent);
    match style {
        OverlayStyle::Modern => Palette {
            background: rgb(18, 22, 30),
            text: rgb(246, 248, 252),
            subtle: rgb(190, 199, 214),
            accent,
        },
        OverlayStyle::Minimal => Palette {
            background: rgb(245, 247, 250),
            text: rgb(24, 28, 35),
            subtle: rgb(74, 84, 102),
            accent,
        },
        OverlayStyle::Bold => Palette {
            background: accent,
            text: rgb(255, 255, 255),
            subtle: rgb(245, 247, 250),
            accent: rgb(255, 255, 255),
        },
    }
}

fn accent_color(accent: OverlayAccent) -> COLORREF {
    match accent {
        OverlayAccent::Blue => rgb(49, 130, 246),
        OverlayAccent::Green => rgb(22, 163, 74),
        OverlayAccent::Amber => rgb(217, 119, 6),
    }
}

fn accent_height(style: OverlayStyle) -> i32 {
    match style {
        OverlayStyle::Minimal => 6,
        OverlayStyle::Modern => 10,
        OverlayStyle::Bold => 0,
    }
}

fn title_font_height(style: OverlayStyle) -> i32 {
    match style {
        OverlayStyle::Minimal => 56,
        OverlayStyle::Modern => 68,
        OverlayStyle::Bold => 76,
    }
}

fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF(red as u32 | ((green as u32) << 8) | ((blue as u32) << 16))
}
