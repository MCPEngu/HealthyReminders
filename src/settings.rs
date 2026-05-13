use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Sender},
        Mutex, MutexGuard, OnceLock,
    },
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{BOOL, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            CreateFontW, GetMonitorInfoW, GetStockObject, GetSysColorBrush, InvalidateRect,
            MonitorFromWindow, COLOR_WINDOW, DEFAULT_GUI_FONT, MONITORINFO,
            MONITOR_DEFAULTTONEAREST,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::GetDpiForWindow,
            Shell::ShellExecuteW,
            WindowsAndMessaging::{
                AppendMenuW, CreateMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DrawMenuBar,
                GetDlgItem, GetDlgItemTextW, GetMenu, GetWindowRect, LoadCursorW, LoadIconW,
                MessageBoxW, RegisterClassW, SendMessageW, SetForegroundWindow, SetMenu,
                SetWindowPos, SetWindowTextW, ShowWindow, BM_GETCHECK, BM_SETCHECK,
                BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_OWNERDRAW, CW_USEDEFAULT, ES_NUMBER, HMENU,
                IDC_ARROW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MENU_ITEM_FLAGS, MF_OWNERDRAW,
                MF_POPUP, MF_SEPARATOR, MF_STRING, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC,
                WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_MEASUREITEM,
                WM_NCACTIVATE, WM_NCPAINT, WM_SETFONT, WM_SETTINGCHANGE, WM_THEMECHANGED,
                WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
                WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
            },
        },
    },
};

use crate::{
    core,
    notifier::NotifierCommand,
    scheduler::{ReminderKind, SchedulerCommand},
};

mod ids;
mod pages;
mod text;
mod theme;

use ids::*;
use pages::{page_control_ids, page_from_index, page_from_nav_id, SettingsPage, ALL_PAGE_IDS};
use text::{
    about_text, about_title, glass_label, settings_menu_title, settings_text, test_success_text,
    water_chart_text, water_logged_text, SettingsMenuTitle,
};

static SETTINGS: OnceLock<Mutex<Option<SettingsState>>> = OnceLock::new();
static MENU_LABELS: OnceLock<Mutex<Vec<Vec<u16>>>> = OnceLock::new();
static SETTINGS_FONTS: OnceLock<Mutex<Vec<SettingsFont>>> = OnceLock::new();
static CLASS_REGISTERED: OnceLock<()> = OnceLock::new();
static ACTIVE_SETTINGS_PAGE: AtomicUsize = AtomicUsize::new(0);

const STANDARD_DPI: u32 = 96;
const COMFORT_DPI: u32 = 120;
const HIGH_RES_MONITOR_WIDTH: i32 = 2300;
const HIGH_RES_MONITOR_HEIGHT: i32 = 1300;

#[derive(Clone, Copy)]
struct SettingsFont {
    dpi: u32,
    handle: isize,
}

pub struct SettingsContext {
    pub config_path: PathBuf,
    pub stats_path: PathBuf,
    pub scheduler_tx: Sender<SchedulerCommand>,
    pub notifier_tx: Sender<NotifierCommand>,
}

#[derive(Clone)]
struct SettingsState {
    hwnd: isize,
    config_path: PathBuf,
    stats_path: PathBuf,
    scheduler_tx: Sender<SchedulerCommand>,
    notifier_tx: Sender<NotifierCommand>,
    theme: core::ThemeMode,
    language: core::Language,
}

pub fn show_settings(ctx: SettingsContext) -> Result<()> {
    let state_lock = SETTINGS.get_or_init(|| Mutex::new(None));

    let existing_state = {
        let mut guard = lock_settings(state_lock)?;
        if let Some(existing) = guard.as_mut() {
            let config = core::load_config(&ctx.config_path);
            existing.config_path = ctx.config_path.clone();
            existing.stats_path = ctx.stats_path.clone();
            existing.scheduler_tx = ctx.scheduler_tx.clone();
            existing.notifier_tx = ctx.notifier_tx.clone();
            existing.theme = config.theme;
            existing.language = config.language;
            Some((existing.clone(), config))
        } else {
            None
        }
    };

    if let Some((existing, config)) = existing_state {
        let hwnd = HWND(existing.hwnd);
        refresh_controls(hwnd, &config, &existing.stats_path);
        apply_theme(hwnd);
        show_settings_window(hwnd);
        return Ok(());
    }

    register_class()?;
    let hwnd = create_window().context("failed to create settings window")?;
    create_controls(hwnd).context("failed to create settings controls")?;

    let config = core::load_config(&ctx.config_path);
    let state = SettingsState {
        hwnd: hwnd.0,
        config_path: ctx.config_path,
        stats_path: ctx.stats_path,
        scheduler_tx: ctx.scheduler_tx,
        notifier_tx: ctx.notifier_tx,
        theme: config.theme,
        language: config.language,
    };

    {
        let mut guard = lock_settings(state_lock)?;
        *guard = Some(state);
    }

    let state = current_state()?;
    refresh_controls(hwnd, &config, &state.stats_path);
    apply_theme(hwnd);

    show_settings_window(hwnd);
    Ok(())
}

pub(crate) fn refresh_autostart_checkbox() {
    if let Ok(state) = current_state() {
        set_checked(
            HWND(state.hwnd),
            ID_AUTOSTART,
            core::autostart_is_enabled().unwrap_or(false),
        );
    }
}

fn register_class() -> Result<()> {
    if CLASS_REGISTERED.get().is_some() {
        return Ok(());
    }

    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let hinstance = HINSTANCE(module.0);
    let class_name = core::wide_null(CLASS_NAME);
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.context("LoadCursorW failed")?;
    let icon = unsafe { LoadIconW(hinstance, PCWSTR(APP_ICON_RESOURCE_ID as *const u16)) }
        .unwrap_or_default();
    let wc = WNDCLASSW {
        lpfnWndProc: Some(settings_wnd_proc),
        hInstance: hinstance,
        hIcon: icon,
        hCursor: cursor,
        hbrBackground: unsafe { GetSysColorBrush(COLOR_WINDOW) },
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };

    let atom = unsafe { RegisterClassW(&wc) };
    if atom == 0 {
        return Err(anyhow!("RegisterClassW failed"));
    }
    let _ = CLASS_REGISTERED.set(());
    Ok(())
}

fn create_window() -> Result<HWND> {
    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let hinstance = HINSTANCE(module.0);
    let class_name = core::wide_null(CLASS_NAME);
    let title = core::wide_null(&format!("{} - Cài đặt", core::APP_NAME));
    let style = WS_OVERLAPPED
        | WS_CAPTION
        | WS_SYSMENU
        | WS_MINIMIZEBOX
        | WS_CLIPCHILDREN
        | WS_CLIPSIBLINGS;

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            SETTINGS_WINDOW_WIDTH,
            SETTINGS_WINDOW_HEIGHT,
            HWND(0),
            HMENU(0),
            hinstance,
            None,
        )
    };

    if hwnd.0 == 0 {
        Err(anyhow!("CreateWindowExW returned null HWND"))
    } else {
        Ok(hwnd)
    }
}

fn show_settings_window(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    normalize_settings_window(hwnd);
    bring_window_to_front(hwnd);
}

fn bring_window_to_front(hwnd: HWND) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND(-1),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let _ = SetWindowPos(
            hwnd,
            HWND(-2),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let _ = SetForegroundWindow(hwnd);
    }
}

fn normalize_settings_window(hwnd: HWND) {
    let (width, height) = settings_window_size(hwnd);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND(0),
            0,
            0,
            width,
            height,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    apply_scaled_fonts(hwnd);
    layout_settings_controls(hwnd);
    apply_settings_page(hwnd, current_settings_page());
    keep_window_on_current_monitor(hwnd);
}

fn keep_window_on_current_monitor(hwnd: HWND) {
    let mut window = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window) }.is_err() {
        return;
    }

    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.0 == 0 {
        return;
    }

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return;
    }

    let work = info.rcWork;
    let width = window.right - window.left;
    let height = window.bottom - window.top;
    let left = clamp_window_axis(window.left, width, work.left, work.right);
    let top = clamp_window_axis(window.top, height, work.top, work.bottom);

    if left != window.left || top != window.top {
        unsafe {
            let _ = SetWindowPos(
                hwnd,
                HWND(0),
                left,
                top,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

fn clamp_window_axis(current: i32, size: i32, min: i32, max: i32) -> i32 {
    if size >= max - min {
        return min;
    }
    current.clamp(min, max - size)
}

fn settings_window_size(hwnd: HWND) -> (i32, i32) {
    (
        scale_value(hwnd, SETTINGS_WINDOW_WIDTH),
        scale_value(hwnd, SETTINGS_WINDOW_HEIGHT),
    )
}

pub(super) fn scale_value(hwnd: HWND, value: i32) -> i32 {
    scale_value_for_dpi(value, effective_ui_dpi(hwnd))
}

fn scale_value_for_dpi(value: i32, dpi: u32) -> i32 {
    let dpi = dpi.max(STANDARD_DPI);
    (((value as i64) * (dpi as i64) + ((STANDARD_DPI / 2) as i64)) / (STANDARD_DPI as i64))
        .clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

fn effective_ui_dpi(hwnd: HWND) -> u32 {
    let window_dpi = unsafe { GetDpiForWindow(hwnd) }.max(STANDARD_DPI);
    if window_dpi <= STANDARD_DPI && is_high_resolution_monitor(hwnd) {
        COMFORT_DPI
    } else {
        STANDARD_DPI
    }
}

fn is_high_resolution_monitor(hwnd: HWND) -> bool {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.0 == 0 {
        return false;
    }

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return false;
    }

    let width = info.rcMonitor.right - info.rcMonitor.left;
    let height = info.rcMonitor.bottom - info.rcMonitor.top;
    width >= HIGH_RES_MONITOR_WIDTH || height >= HIGH_RES_MONITOR_HEIGHT
}

#[derive(Clone, Copy)]
struct ControlSpec {
    class_name: &'static str,
    text: &'static str,
    style: WINDOW_STYLE,
    id: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

macro_rules! create_control {
    ($parent:expr, $hinstance:expr, $class_name:expr, $text:expr, $style:expr, $id:expr, $x:expr, $y:expr, $width:expr, $height:expr $(,)?) => {
        create_control_from_spec(
            $parent,
            $hinstance,
            ControlSpec {
                class_name: $class_name,
                text: $text,
                style: $style,
                id: $id,
                x: $x,
                y: $y,
                width: $width,
                height: $height,
            },
        )
    };
}

fn create_controls(parent: HWND) -> Result<()> {
    let module = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW failed")?;
    let hinstance = HINSTANCE(module.0);
    let label_style = WS_CHILD | WS_VISIBLE;
    let group_style = label_style;
    let edit_style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | style_bits(ES_NUMBER);
    let time_edit_style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER;
    let checkbox_style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | style_bits(BS_AUTOCHECKBOX);
    let button_style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | style_bits(BS_OWNERDRAW);
    let combo_style =
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | style_bits(CBS_DROPDOWNLIST_VALUE);

    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Hôm nay",
        group_style,
        ID_SECTION_TODAY,
        24,
        16,
        120,
        20,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "",
        label_style,
        ID_DASHBOARD,
        40,
        44,
        430,
        54,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "",
        label_style,
        ID_WATER_CHART,
        500,
        44,
        200,
        64,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Nhắc nhở",
        button_style,
        ID_NAV_REMINDERS,
        24,
        108,
        128,
        30,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Nước",
        button_style,
        ID_NAV_WATER,
        160,
        108,
        104,
        30,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Lịch",
        button_style,
        ID_NAV_SCHEDULE,
        272,
        108,
        120,
        30,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Màn hình",
        button_style,
        ID_NAV_DISPLAY,
        400,
        108,
        136,
        30,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Ứng dụng",
        button_style,
        ID_NAV_APP,
        544,
        108,
        120,
        30,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Nhắc nhở",
        group_style,
        ID_SECTION_REMINDERS,
        24,
        136,
        140,
        20,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Uống nước",
        checkbox_style,
        ID_WATER_ENABLED,
        40,
        162,
        110,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Nghỉ mắt",
        checkbox_style,
        ID_EYE_ENABLED,
        148,
        162,
        98,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Đứng dậy",
        checkbox_style,
        ID_MOVEMENT_ENABLED,
        250,
        162,
        110,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Chu kỳ uống nước (phút)",
        label_style,
        ID_LABEL_WATER_MINUTES,
        40,
        196,
        230,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_WATER_MINUTES,
        294,
        192,
        54,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Chu kỳ nghỉ mắt (phút)",
        label_style,
        ID_LABEL_EYE_MINUTES,
        40,
        226,
        230,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_EYE_MINUTES,
        294,
        222,
        54,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Đứng dậy sau (phút)",
        label_style,
        ID_LABEL_STAND_MINUTES,
        40,
        256,
        230,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_STAND_MINUTES,
        294,
        252,
        54,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Thời gian đứng dậy (phút)",
        label_style,
        ID_LABEL_STAND_DURATION,
        40,
        286,
        230,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_STAND_DURATION,
        294,
        282,
        54,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Nghỉ mắt countdown (giây)",
        label_style,
        ID_LABEL_EYE_COUNTDOWN,
        40,
        316,
        230,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_EYE_COUNTDOWN,
        294,
        312,
        54,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Nghỉ mắt im lặng",
        checkbox_style,
        ID_EYE_SILENT,
        40,
        338,
        130,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Gợi ý vận động",
        checkbox_style,
        ID_MOVEMENT_RANDOM,
        172,
        338,
        185,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Nước",
        group_style,
        ID_SECTION_WATER,
        388,
        136,
        120,
        20,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Goal tự động",
        checkbox_style,
        ID_WATER_GOAL_AUTO,
        404,
        162,
        150,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Ghi 1 ly",
        button_style,
        ID_LOG_WATER,
        610,
        158,
        92,
        30,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Cân nặng (kg)",
        label_style,
        ID_LABEL_WEIGHT,
        404,
        196,
        190,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_WATER_WEIGHT,
        632,
        192,
        70,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Giới tính",
        label_style,
        ID_LABEL_GENDER,
        404,
        226,
        190,
        22,
    )?;
    let gender_combo = create_control!(
        parent,
        hinstance,
        "COMBOBOX",
        "",
        combo_style,
        ID_WATER_GENDER,
        632,
        222,
        70,
        120,
    )?;
    add_combo_items(gender_combo, &["Khác", "Nam", "Nữ"]);
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Đơn vị",
        label_style,
        ID_LABEL_UNIT,
        404,
        256,
        190,
        22,
    )?;
    let unit_combo = create_control!(
        parent,
        hinstance,
        "COMBOBOX",
        "",
        combo_style,
        ID_WATER_UNIT,
        632,
        252,
        70,
        120,
    )?;
    add_combo_items(unit_combo, &["ml", "oz"]);
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Goal thủ công",
        label_style,
        ID_LABEL_GOAL_OVERRIDE,
        404,
        286,
        190,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_WATER_GOAL_OVERRIDE,
        632,
        282,
        70,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Một ly nước (ml)",
        label_style,
        ID_LABEL_DEFAULT_GLASS,
        404,
        316,
        190,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_DEFAULT_GLASS,
        632,
        312,
        70,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Lịch làm việc",
        group_style,
        ID_SECTION_SCHEDULE,
        24,
        376,
        160,
        20,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Chỉ nhắc trong giờ làm",
        checkbox_style,
        ID_WORK_ENABLED,
        40,
        402,
        220,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Không nhắc khi nghỉ trưa",
        checkbox_style,
        ID_LUNCH_ENABLED,
        40,
        432,
        230,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Giờ làm (HH:MM)",
        label_style,
        ID_LABEL_WORK_HOURS,
        40,
        464,
        110,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        time_edit_style,
        ID_WORK_START,
        168,
        460,
        70,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "-",
        label_style,
        ID_LABEL_WORK_SEPARATOR,
        246,
        464,
        16,
        20,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        time_edit_style,
        ID_WORK_END,
        264,
        460,
        70,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Nghỉ trưa (HH:MM)",
        label_style,
        ID_LABEL_LUNCH_HOURS,
        40,
        494,
        110,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        time_edit_style,
        ID_LUNCH_START,
        168,
        490,
        70,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "-",
        label_style,
        ID_LABEL_LUNCH_SEPARATOR,
        246,
        494,
        16,
        20,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        time_edit_style,
        ID_LUNCH_END,
        264,
        490,
        70,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Hiển thị khi nhắc",
        group_style,
        ID_SECTION_DISPLAY,
        388,
        376,
        180,
        20,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Full-screen reminder",
        checkbox_style,
        ID_FULLSCREEN_REMINDERS,
        404,
        402,
        180,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Focus countdown",
        checkbox_style,
        ID_FOCUS_MODE,
        404,
        432,
        150,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Focus giây",
        label_style,
        ID_LABEL_FOCUS_SECONDS,
        558,
        434,
        70,
        22,
    )?;
    create_control!(
        parent,
        hinstance,
        "EDIT",
        "",
        edit_style,
        ID_FOCUS_SECONDS,
        650,
        430,
        52,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Overlay style",
        label_style,
        ID_LABEL_OVERLAY_STYLE,
        404,
        466,
        110,
        22,
    )?;
    let overlay_style_combo = create_control!(
        parent,
        hinstance,
        "COMBOBOX",
        "",
        combo_style,
        ID_OVERLAY_STYLE,
        520,
        462,
        110,
        120,
    )?;
    add_combo_items(overlay_style_combo, &["Modern", "Minimal", "Bold"]);
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Snooze họp (phút)",
        label_style,
        ID_LABEL_SNOOZE,
        404,
        496,
        110,
        22,
    )?;
    let movement_snooze_combo = create_control!(
        parent,
        hinstance,
        "COMBOBOX",
        "",
        combo_style,
        ID_MOVEMENT_SNOOZE,
        520,
        492,
        60,
        120,
    )?;
    add_combo_items(movement_snooze_combo, &["5", "10", "15"]);
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Accent",
        label_style,
        ID_LABEL_OVERLAY_ACCENT,
        592,
        496,
        52,
        22,
    )?;
    let overlay_accent_combo = create_control!(
        parent,
        hinstance,
        "COMBOBOX",
        "",
        combo_style,
        ID_OVERLAY_ACCENT,
        644,
        492,
        58,
        120,
    )?;
    add_combo_items(overlay_accent_combo, &["Blue", "Green", "Amber"]);
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Ứng dụng",
        group_style,
        ID_SECTION_APP,
        24,
        540,
        120,
        20,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Âm thanh thông báo",
        checkbox_style,
        ID_SOUND,
        40,
        566,
        190,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Tự chạy cùng Windows",
        checkbox_style,
        ID_AUTOSTART,
        242,
        566,
        210,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Minimal mode",
        checkbox_style,
        ID_MINIMAL_MODE,
        520,
        566,
        150,
        24,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Nền",
        label_style,
        ID_LABEL_THEME,
        40,
        602,
        80,
        22,
    )?;
    let theme_combo = create_control!(
        parent,
        hinstance,
        "COMBOBOX",
        "",
        combo_style,
        ID_THEME,
        112,
        598,
        180,
        120,
    )?;
    add_combo_items(theme_combo, &["Theo hệ thống", "Sáng", "Tối"]);
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "Ngôn ngữ",
        label_style,
        ID_LABEL_LANGUAGE,
        328,
        602,
        90,
        22,
    )?;
    let language_combo = create_control!(
        parent,
        hinstance,
        "COMBOBOX",
        "",
        combo_style,
        ID_LANGUAGE,
        420,
        598,
        150,
        120,
    )?;
    add_combo_items(language_combo, &["Tiếng Việt", "English"]);
    create_control!(
        parent,
        hinstance,
        "BUTTON",
        "Lưu",
        button_style | style_bits(BS_DEFPUSHBUTTON),
        ID_SAVE,
        40,
        696,
        90,
        30,
    )?;
    create_control!(
        parent,
        hinstance,
        "STATIC",
        "",
        label_style,
        ID_STATUS,
        24,
        772,
        696,
        22,
    )?;

    layout_settings_controls(parent);
    apply_settings_page(parent, current_settings_page());

    Ok(())
}

fn layout_settings_controls(parent: HWND) {
    for (id, x, y, width, height) in [
        (ID_SECTION_TODAY, 24, 16, 120, 20),
        (ID_DASHBOARD, 40, 44, 410, 90),
        (ID_WATER_CHART, 480, 44, 240, 104),
        (ID_SAVE, 40, 516, 100, 30),
        (ID_STATUS, 24, 566, 696, 22),
    ] {
        move_control(parent, id, x, y, width, height);
    }
    set_controls_visible(parent, NAV_CONTROL_IDS, false);

    for (id, x, y, width, height) in [
        (ID_SECTION_REMINDERS, 24, 154, 180, 20),
        (ID_WATER_ENABLED, 40, 194, 130, 24),
        (ID_EYE_ENABLED, 200, 194, 130, 24),
        (ID_MOVEMENT_ENABLED, 360, 194, 130, 24),
        (ID_LABEL_WATER_MINUTES, 40, 240, 260, 22),
        (ID_WATER_MINUTES, 340, 236, 70, 24),
        (ID_LABEL_EYE_MINUTES, 40, 278, 260, 22),
        (ID_EYE_MINUTES, 340, 274, 70, 24),
        (ID_LABEL_STAND_MINUTES, 40, 316, 260, 22),
        (ID_STAND_MINUTES, 340, 312, 70, 24),
        (ID_LABEL_STAND_DURATION, 40, 354, 260, 22),
        (ID_STAND_DURATION, 340, 350, 70, 24),
        (ID_LABEL_EYE_COUNTDOWN, 40, 392, 260, 22),
        (ID_EYE_COUNTDOWN, 340, 388, 70, 24),
        (ID_EYE_SILENT, 40, 436, 180, 24),
        (ID_MOVEMENT_RANDOM, 240, 436, 180, 24),
    ] {
        move_control(parent, id, x, y, width, height);
    }

    for (id, x, y, width, height) in [
        (ID_SECTION_WATER, 24, 154, 160, 20),
        (ID_WATER_GOAL_AUTO, 40, 194, 250, 24),
        (ID_LOG_WATER, 340, 190, 140, 30),
        (ID_LABEL_WEIGHT, 40, 240, 260, 22),
        (ID_WATER_WEIGHT, 340, 236, 140, 24),
        (ID_LABEL_GENDER, 40, 278, 260, 22),
        (ID_WATER_GENDER, 340, 274, 140, 120),
        (ID_LABEL_UNIT, 40, 316, 260, 22),
        (ID_WATER_UNIT, 340, 312, 140, 120),
        (ID_LABEL_GOAL_OVERRIDE, 40, 354, 260, 22),
        (ID_WATER_GOAL_OVERRIDE, 340, 350, 140, 24),
        (ID_LABEL_DEFAULT_GLASS, 40, 392, 260, 22),
        (ID_DEFAULT_GLASS, 340, 388, 140, 24),
    ] {
        move_control(parent, id, x, y, width, height);
    }

    for (id, x, y, width, height) in [
        (ID_SECTION_SCHEDULE, 24, 154, 160, 20),
        (ID_WORK_ENABLED, 40, 194, 260, 24),
        (ID_LUNCH_ENABLED, 40, 232, 260, 24),
        (ID_LABEL_WORK_HOURS, 40, 284, 180, 22),
        (ID_WORK_START, 240, 280, 82, 24),
        (ID_LABEL_WORK_SEPARATOR, 332, 284, 16, 20),
        (ID_WORK_END, 356, 280, 82, 24),
        (ID_LABEL_LUNCH_HOURS, 40, 326, 180, 22),
        (ID_LUNCH_START, 240, 322, 82, 24),
        (ID_LABEL_LUNCH_SEPARATOR, 332, 326, 16, 20),
        (ID_LUNCH_END, 356, 322, 82, 24),
    ] {
        move_control(parent, id, x, y, width, height);
    }

    for (id, x, y, width, height) in [
        (ID_SECTION_DISPLAY, 24, 154, 160, 20),
        (ID_FULLSCREEN_REMINDERS, 40, 194, 240, 24),
        (ID_FOCUS_MODE, 40, 232, 220, 24),
        (ID_LABEL_FOCUS_SECONDS, 40, 274, 180, 22),
        (ID_FOCUS_SECONDS, 240, 270, 90, 24),
        (ID_LABEL_OVERLAY_STYLE, 40, 326, 180, 22),
        (ID_OVERLAY_STYLE, 240, 322, 170, 120),
        (ID_LABEL_SNOOZE, 40, 368, 180, 22),
        (ID_MOVEMENT_SNOOZE, 240, 364, 90, 120),
        (ID_LABEL_OVERLAY_ACCENT, 40, 410, 180, 22),
        (ID_OVERLAY_ACCENT, 240, 406, 120, 120),
    ] {
        move_control(parent, id, x, y, width, height);
    }

    for (id, x, y, width, height) in [
        (ID_SECTION_APP, 24, 154, 160, 20),
        (ID_SOUND, 40, 194, 220, 24),
        (ID_AUTOSTART, 40, 232, 240, 24),
        (ID_MINIMAL_MODE, 40, 270, 180, 24),
        (ID_LABEL_THEME, 40, 322, 180, 22),
        (ID_THEME, 240, 318, 200, 120),
        (ID_LABEL_LANGUAGE, 40, 364, 180, 22),
        (ID_LANGUAGE, 240, 360, 200, 120),
    ] {
        move_control(parent, id, x, y, width, height);
    }
}

fn move_control(parent: HWND, id: i32, x: i32, y: i32, width: i32, height: i32) {
    let hwnd = unsafe { GetDlgItem(parent, id) };
    if hwnd.0 == 0 {
        return;
    }
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND(0),
            scale_value(parent, x),
            scale_value(parent, y),
            scale_value(parent, width),
            scale_value(parent, height),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn apply_settings_page(parent: HWND, page: SettingsPage) {
    ACTIVE_SETTINGS_PAGE.store(page as usize, Ordering::Release);
    set_controls_visible(parent, ALL_PAGE_IDS, false);
    set_controls_visible(parent, page_control_ids(page), true);
    unsafe {
        let _ = InvalidateRect(parent, None, BOOL(1));
    }
}

fn set_controls_visible(parent: HWND, ids: &[i32], visible: bool) {
    let command = if visible { SW_SHOW } else { SW_HIDE };
    for id in ids {
        let hwnd = unsafe { GetDlgItem(parent, *id) };
        if hwnd.0 != 0 {
            unsafe {
                let _ = ShowWindow(hwnd, command);
            }
        }
    }
}

fn current_settings_page() -> SettingsPage {
    page_from_index(ACTIVE_SETTINGS_PAGE.load(Ordering::Acquire))
}

fn create_control_from_spec(parent: HWND, hinstance: HINSTANCE, spec: ControlSpec) -> Result<HWND> {
    let class_name = core::wide_null(spec.class_name);
    let text = core::wide_null(spec.text);
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(text.as_ptr()),
            spec.style,
            scale_value(parent, spec.x),
            scale_value(parent, spec.y),
            scale_value(parent, spec.width),
            scale_value(parent, spec.height),
            parent,
            HMENU(spec.id as isize),
            hinstance,
            None,
        )
    };

    if hwnd.0 == 0 {
        return Err(anyhow!("CreateWindowExW child control failed"));
    }

    set_control_font(parent, hwnd);
    Ok(hwnd)
}

fn add_combo_items(hwnd: HWND, items: &[&str]) {
    for item in items {
        let text = core::wide_null(item);
        unsafe {
            let _ = SendMessageW(
                hwnd,
                CB_ADDSTRING,
                WPARAM(0),
                LPARAM(text.as_ptr() as isize),
            );
        }
    }
}

fn apply_language(parent: HWND, language: core::Language) {
    let text = settings_text(language);
    set_window_text(parent, text.window_title);
    if let Err(error) = install_settings_menu(parent, language) {
        log::warn!("cannot install settings menu: {error:#}");
    }
    set_control_text(parent, ID_SECTION_TODAY, text.section_today);
    set_control_text(parent, ID_SECTION_REMINDERS, text.section_reminders);
    set_control_text(parent, ID_SECTION_WATER, text.section_water);
    set_control_text(parent, ID_SECTION_SCHEDULE, text.section_schedule);
    set_control_text(parent, ID_SECTION_DISPLAY, text.section_display);
    set_control_text(parent, ID_SECTION_APP, text.section_app);
    set_control_text(parent, ID_NAV_REMINDERS, text.section_reminders);
    set_control_text(parent, ID_NAV_WATER, text.section_water);
    set_control_text(parent, ID_NAV_SCHEDULE, text.section_schedule);
    set_control_text(parent, ID_NAV_DISPLAY, text.section_display);
    set_control_text(parent, ID_NAV_APP, text.section_app);
    set_control_text(parent, ID_WATER_ENABLED, text.water_enabled);
    set_control_text(parent, ID_EYE_ENABLED, text.eye_enabled);
    set_control_text(parent, ID_MOVEMENT_ENABLED, text.movement_enabled);
    set_control_text(parent, ID_WATER_GOAL_AUTO, text.auto_goal);
    set_control_text(parent, ID_LABEL_WEIGHT, text.weight);
    set_control_text(parent, ID_LABEL_GENDER, text.gender);
    set_control_text(parent, ID_LABEL_UNIT, text.unit);
    set_control_text(parent, ID_LABEL_GOAL_OVERRIDE, text.manual_goal);
    set_control_text(parent, ID_LOG_WATER, text.log_water);
    set_control_text(parent, ID_LABEL_WATER_MINUTES, text.water_minutes);
    set_control_text(parent, ID_LABEL_EYE_MINUTES, text.eye_minutes);
    set_control_text(parent, ID_LABEL_STAND_MINUTES, text.stand_minutes);
    set_control_text(parent, ID_LABEL_STAND_DURATION, text.stand_duration);
    set_control_text(parent, ID_LABEL_DEFAULT_GLASS, text.default_glass);
    set_control_text(parent, ID_LABEL_EYE_COUNTDOWN, text.eye_countdown);
    set_control_text(parent, ID_EYE_SILENT, text.eye_silent);
    set_control_text(parent, ID_MOVEMENT_RANDOM, text.movement_random);
    set_control_text(parent, ID_LABEL_SNOOZE, text.snooze);
    set_control_text(parent, ID_WORK_ENABLED, text.work_enabled);
    set_control_text(parent, ID_LABEL_WORK_HOURS, text.work_hours);
    set_control_text(parent, ID_LUNCH_ENABLED, text.lunch_enabled);
    set_control_text(parent, ID_LABEL_LUNCH_HOURS, text.lunch_hours);
    set_control_text(parent, ID_SOUND, text.sound);
    set_control_text(parent, ID_AUTOSTART, text.autostart);
    set_control_text(parent, ID_MINIMAL_MODE, text.minimal_mode);
    set_control_text(parent, ID_LABEL_THEME, text.theme);
    set_control_text(parent, ID_LABEL_LANGUAGE, text.language);
    set_control_text(parent, ID_FULLSCREEN_REMINDERS, text.full_screen);
    set_control_text(parent, ID_FOCUS_MODE, text.focus_mode);
    set_control_text(parent, ID_LABEL_FOCUS_SECONDS, text.focus_seconds);
    set_control_text(parent, ID_LABEL_OVERLAY_STYLE, text.overlay_style);
    set_control_text(parent, ID_LABEL_OVERLAY_ACCENT, text.overlay_accent);
    set_control_text(parent, ID_SAVE, text.save);

    reset_combo_items(
        parent,
        ID_THEME,
        &text.theme_items,
        combo_index(parent, ID_THEME),
    );
    reset_combo_items(
        parent,
        ID_WATER_GENDER,
        &text.gender_items,
        combo_index(parent, ID_WATER_GENDER),
    );
    reset_combo_items(
        parent,
        ID_LANGUAGE,
        &text.language_items,
        Some(language_index(language)),
    );
    reset_combo_items(
        parent,
        ID_OVERLAY_STYLE,
        &text.overlay_style_items,
        combo_index(parent, ID_OVERLAY_STYLE),
    );
    reset_combo_items(
        parent,
        ID_OVERLAY_ACCENT,
        &text.overlay_accent_items,
        combo_index(parent, ID_OVERLAY_ACCENT),
    );
}

fn install_settings_menu(hwnd: HWND, language: core::Language) -> Result<()> {
    let text = settings_text(language);
    let theme = current_theme(hwnd);
    let mut menu_labels = Vec::new();
    let menu = unsafe { CreateMenu() }.context("CreateMenu failed")?;
    let settings_menu = unsafe { CreateMenu() }.context("CreateMenu Settings failed")?;
    let view_menu = unsafe { CreateMenu() }.context("CreateMenu View failed")?;
    let tools_menu = unsafe { CreateMenu() }.context("CreateMenu Tools failed")?;
    let help_menu = unsafe { CreateMenu() }.context("CreateMenu Help failed")?;

    append_owner_menu_item(settings_menu, ID_SAVE as usize, text.save, &mut menu_labels)?;
    append_owner_menu_item(
        settings_menu,
        ID_NAV_APP as usize,
        text.section_app,
        &mut menu_labels,
    )?;
    append_menu_separator(settings_menu)?;
    append_owner_menu_item(settings_menu, ID_HIDE as usize, text.hide, &mut menu_labels)?;

    append_owner_menu_item(
        view_menu,
        ID_NAV_REMINDERS as usize,
        text.section_reminders,
        &mut menu_labels,
    )?;
    append_owner_menu_item(
        view_menu,
        ID_NAV_WATER as usize,
        text.section_water,
        &mut menu_labels,
    )?;
    append_owner_menu_item(
        view_menu,
        ID_NAV_SCHEDULE as usize,
        text.section_schedule,
        &mut menu_labels,
    )?;
    append_owner_menu_item(
        view_menu,
        ID_NAV_DISPLAY as usize,
        text.section_display,
        &mut menu_labels,
    )?;

    append_owner_menu_item(
        tools_menu,
        ID_TEST_WATER as usize,
        text.test_water,
        &mut menu_labels,
    )?;
    append_owner_menu_item(
        tools_menu,
        ID_TEST_EYES as usize,
        text.test_eyes,
        &mut menu_labels,
    )?;
    append_owner_menu_item(
        tools_menu,
        ID_TEST_STAND as usize,
        text.test_stand,
        &mut menu_labels,
    )?;
    append_owner_menu_item(
        tools_menu,
        ID_TEST_WORK as usize,
        text.test_work,
        &mut menu_labels,
    )?;
    append_menu_separator(tools_menu)?;
    append_owner_menu_item(
        tools_menu,
        ID_LOG_WATER as usize,
        text.log_water,
        &mut menu_labels,
    )?;

    append_owner_menu_item(help_menu, ID_ABOUT as usize, text.about, &mut menu_labels)?;
    append_owner_menu_item(
        help_menu,
        ID_CHECK_UPDATES as usize,
        text.updates,
        &mut menu_labels,
    )?;

    theme::apply_menu_theme(settings_menu, theme);
    theme::apply_menu_theme(view_menu, theme);
    theme::apply_menu_theme(tools_menu, theme);
    theme::apply_menu_theme(help_menu, theme);
    theme::apply_menu_theme(menu, theme);

    append_menu_item(
        menu,
        MF_POPUP | MF_STRING,
        view_menu.0 as usize,
        settings_menu_title(language, SettingsMenuTitle::View),
    )?;
    append_menu_item(
        menu,
        MF_POPUP | MF_STRING,
        tools_menu.0 as usize,
        settings_menu_title(language, SettingsMenuTitle::Tools),
    )?;
    append_menu_item(
        menu,
        MF_POPUP | MF_STRING,
        settings_menu.0 as usize,
        settings_menu_title(language, SettingsMenuTitle::Settings),
    )?;
    append_menu_item(
        menu,
        MF_POPUP | MF_STRING,
        help_menu.0 as usize,
        settings_menu_title(language, SettingsMenuTitle::Help),
    )?;

    let old_menu = unsafe { GetMenu(hwnd) };
    unsafe {
        SetMenu(hwnd, menu).context("SetMenu failed")?;
        replace_menu_labels(menu_labels);
        DrawMenuBar(hwnd).context("DrawMenuBar failed")?;
        if old_menu.0 != 0 {
            let _ = DestroyMenu(old_menu);
        }
    }
    Ok(())
}

fn append_menu_item(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, text: &str) -> Result<()> {
    let text = core::wide_null(text);
    unsafe { AppendMenuW(menu, flags, id, PCWSTR(text.as_ptr())) }.context("AppendMenuW failed")
}

fn append_owner_menu_item(
    menu: HMENU,
    id: usize,
    text: &str,
    menu_labels: &mut Vec<Vec<u16>>,
) -> Result<()> {
    menu_labels.push(core::wide_null(text));
    let item_data = menu_labels
        .last()
        .map(|label| label.as_ptr())
        .unwrap_or(std::ptr::null());
    unsafe { AppendMenuW(menu, MF_OWNERDRAW, id, PCWSTR(item_data)) }
        .context("AppendMenuW owner-draw failed")
}

fn append_menu_separator(menu: HMENU) -> Result<()> {
    unsafe {
        AppendMenuW(
            menu,
            MF_SEPARATOR | MF_OWNERDRAW,
            0,
            PCWSTR(std::ptr::null()),
        )
    }
    .context("AppendMenuW separator failed")
}

fn replace_menu_labels(labels: Vec<Vec<u16>>) {
    let lock = MENU_LABELS.get_or_init(|| Mutex::new(Vec::new()));
    match lock.lock() {
        Ok(mut guard) => *guard = labels,
        Err(poisoned) => *poisoned.into_inner() = labels,
    }
}

fn reset_combo_items(parent: HWND, id: i32, items: &[&str], selected: Option<usize>) {
    let hwnd = unsafe { GetDlgItem(parent, id) };
    if hwnd.0 == 0 {
        return;
    }
    unsafe {
        let _ = SendMessageW(hwnd, CB_RESETCONTENT, WPARAM(0), LPARAM(0));
    }
    add_combo_items(hwnd, items);
    if let Some(index) = selected {
        set_combo_index(parent, id, index.min(items.len().saturating_sub(1)));
    }
}

fn refresh_controls(hwnd: HWND, config: &core::AppConfig, stats_path: &Path) {
    update_cached_appearance(config.theme, config.language);
    set_checked(hwnd, ID_WATER_ENABLED, config.water_enabled);
    set_checked(hwnd, ID_EYE_ENABLED, config.eye_enabled);
    set_checked(hwnd, ID_MOVEMENT_ENABLED, config.movement_enabled);
    set_edit_u64(hwnd, ID_WATER_MINUTES, config.water_interval_minutes);
    set_edit_u64(hwnd, ID_EYE_MINUTES, config.eye_interval_minutes);
    set_edit_u64(hwnd, ID_STAND_MINUTES, config.stand_interval_minutes);
    set_edit_u64(hwnd, ID_STAND_DURATION, config.stand_duration_minutes);
    set_edit_u64(hwnd, ID_DEFAULT_GLASS, config.default_glass_ml);
    set_checked(hwnd, ID_WATER_GOAL_AUTO, config.water_goal_auto);
    set_edit_u64(hwnd, ID_WATER_WEIGHT, config.water_weight_kg);
    set_combo_index(hwnd, ID_WATER_GENDER, gender_index(config.water_gender));
    set_combo_index(hwnd, ID_WATER_UNIT, water_unit_index(config.water_unit));
    set_edit_u64(hwnd, ID_WATER_GOAL_OVERRIDE, config.water_goal_ml_override);
    set_edit_u64(hwnd, ID_EYE_COUNTDOWN, config.eye_countdown_seconds);
    set_checked(hwnd, ID_EYE_SILENT, config.eye_silent_mode);
    set_checked(hwnd, ID_MOVEMENT_RANDOM, config.movement_random_suggestion);
    set_combo_index(
        hwnd,
        ID_MOVEMENT_SNOOZE,
        movement_snooze_index(config.movement_snooze_minutes),
    );
    set_checked(hwnd, ID_WORK_ENABLED, config.work_hours_enabled);
    set_edit_hhmm(hwnd, ID_WORK_START, config.work_start_minutes);
    set_edit_hhmm(hwnd, ID_WORK_END, config.work_end_minutes);
    set_checked(hwnd, ID_LUNCH_ENABLED, config.lunch_break_enabled);
    set_edit_hhmm(hwnd, ID_LUNCH_START, config.lunch_start_minutes);
    set_edit_hhmm(hwnd, ID_LUNCH_END, config.lunch_end_minutes);
    set_combo_index(hwnd, ID_THEME, theme_index(config.theme));
    set_combo_index(hwnd, ID_LANGUAGE, language_index(config.language));
    set_checked(hwnd, ID_MINIMAL_MODE, config.minimal_mode);
    set_checked(hwnd, ID_FULLSCREEN_REMINDERS, config.full_screen_reminders);
    set_checked(hwnd, ID_FOCUS_MODE, config.focus_mode_enabled);
    set_edit_u64(hwnd, ID_FOCUS_SECONDS, config.focus_countdown_seconds);
    set_combo_index(
        hwnd,
        ID_OVERLAY_STYLE,
        overlay_style_index(config.overlay_style),
    );
    set_combo_index(
        hwnd,
        ID_OVERLAY_ACCENT,
        overlay_accent_index(config.overlay_accent),
    );
    set_checked(hwnd, ID_SOUND, config.sound);
    set_checked(
        hwnd,
        ID_AUTOSTART,
        core::autostart_is_enabled().unwrap_or(false),
    );
    apply_language(hwnd, config.language);
    refresh_dashboard(hwnd, stats_path, config);
}

fn save_from_window(hwnd: HWND) -> Result<()> {
    let state = current_state()?;
    let mut config = core::load_config(&state.config_path);
    config.water_enabled = is_checked(hwnd, ID_WATER_ENABLED);
    config.eye_enabled = is_checked(hwnd, ID_EYE_ENABLED);
    config.movement_enabled = is_checked(hwnd, ID_MOVEMENT_ENABLED);
    config.water_interval_minutes = read_stepped_u64(
        hwnd,
        ID_WATER_MINUTES,
        config.water_interval_minutes,
        5,
        60,
        5,
    );
    config.eye_interval_minutes =
        read_stepped_u64(hwnd, ID_EYE_MINUTES, config.eye_interval_minutes, 5, 60, 5);
    config.stand_interval_minutes = read_stepped_u64(
        hwnd,
        ID_STAND_MINUTES,
        config.stand_interval_minutes,
        15,
        60,
        5,
    );
    config.stand_duration_minutes = read_u64(
        hwnd,
        ID_STAND_DURATION,
        config.stand_duration_minutes,
        1,
        180,
    );
    config.default_glass_ml = read_u64(hwnd, ID_DEFAULT_GLASS, config.default_glass_ml, 50, 2000);
    config.water_goal_auto = is_checked(hwnd, ID_WATER_GOAL_AUTO);
    config.water_weight_kg = read_u64(hwnd, ID_WATER_WEIGHT, config.water_weight_kg, 20, 250);
    config.water_gender = read_gender(hwnd, config.water_gender);
    config.water_unit = read_water_unit(hwnd, config.water_unit);
    config.water_goal_ml_override = read_u64(
        hwnd,
        ID_WATER_GOAL_OVERRIDE,
        config.water_goal_ml_override,
        500,
        6000,
    );
    config.eye_countdown_seconds =
        read_u64(hwnd, ID_EYE_COUNTDOWN, config.eye_countdown_seconds, 10, 60);
    config.eye_silent_mode = is_checked(hwnd, ID_EYE_SILENT);
    config.movement_random_suggestion = is_checked(hwnd, ID_MOVEMENT_RANDOM);
    config.movement_snooze_minutes = read_movement_snooze(hwnd, config.movement_snooze_minutes);
    config.idle_trim_seconds = 60;
    config.work_hours_enabled = is_checked(hwnd, ID_WORK_ENABLED);
    config.work_start_minutes = read_hhmm(hwnd, ID_WORK_START, config.work_start_minutes);
    config.work_end_minutes = read_hhmm(hwnd, ID_WORK_END, config.work_end_minutes);
    config.lunch_break_enabled = is_checked(hwnd, ID_LUNCH_ENABLED);
    config.lunch_start_minutes = read_hhmm(hwnd, ID_LUNCH_START, config.lunch_start_minutes);
    config.lunch_end_minutes = read_hhmm(hwnd, ID_LUNCH_END, config.lunch_end_minutes);
    config.theme = read_theme(hwnd, config.theme);
    config.language = read_language(hwnd, config.language);
    config.minimal_mode = is_checked(hwnd, ID_MINIMAL_MODE);
    config.full_screen_reminders = is_checked(hwnd, ID_FULLSCREEN_REMINDERS);
    config.focus_mode_enabled = is_checked(hwnd, ID_FOCUS_MODE);
    config.focus_countdown_seconds = read_u64(
        hwnd,
        ID_FOCUS_SECONDS,
        config.focus_countdown_seconds,
        5,
        300,
    );
    config.eye_countdown_seconds =
        read_u64(hwnd, ID_EYE_COUNTDOWN, config.eye_countdown_seconds, 10, 60);
    config.eye_silent_mode = is_checked(hwnd, ID_EYE_SILENT);
    config.movement_random_suggestion = is_checked(hwnd, ID_MOVEMENT_RANDOM);
    config.movement_snooze_minutes = read_movement_snooze(hwnd, config.movement_snooze_minutes);
    config.overlay_style = read_overlay_style(hwnd, config.overlay_style);
    config.overlay_accent = read_overlay_accent(hwnd, config.overlay_accent);
    config.sound = is_checked(hwnd, ID_SOUND);

    let autostart_enabled = is_checked(hwnd, ID_AUTOSTART);
    core::save_config(&state.config_path, &config)?;
    core::set_autostart(autostart_enabled)?;
    update_cached_appearance(config.theme, config.language);
    crate::preview_tray_language(config.language);
    crate::preview_tray_autostart(autostart_enabled);
    crate::request_tray_refresh();
    let _ = state.scheduler_tx.send(SchedulerCommand::Reload(config));
    let config = core::load_config(&state.config_path);
    apply_language(hwnd, config.language);
    refresh_dashboard(hwnd, &state.stats_path, &config);
    apply_theme(hwnd);
    core::trim_memory();
    Ok(())
}

fn log_water_from_window(hwnd: HWND) -> Result<()> {
    let state = current_state()?;
    let mut config = core::load_config(&state.config_path);
    config.language = current_language(hwnd);
    config.default_glass_ml = read_u64(hwnd, ID_DEFAULT_GLASS, config.default_glass_ml, 50, 2000);
    config.water_goal_auto = is_checked(hwnd, ID_WATER_GOAL_AUTO);
    config.water_weight_kg = read_u64(hwnd, ID_WATER_WEIGHT, config.water_weight_kg, 20, 250);
    config.water_gender = read_gender(hwnd, config.water_gender);
    config.water_unit = read_water_unit(hwnd, config.water_unit);
    config.water_goal_ml_override = read_u64(
        hwnd,
        ID_WATER_GOAL_OVERRIDE,
        config.water_goal_ml_override,
        500,
        6000,
    );

    let amount = config.default_glass_ml.max(1);
    core::record_activity(&state.stats_path, core::ActivityKind::Water { ml: amount })?;
    refresh_dashboard(hwnd, &state.stats_path, &config);
    set_status(
        hwnd,
        &water_logged_text(
            config.language,
            &core::format_water_volume(amount, config.water_unit),
        ),
    );
    Ok(())
}

fn send_test(kind: ReminderKind, hwnd: HWND) -> Result<()> {
    let state = current_state()?;
    let mut config = core::load_config(&state.config_path);
    config.language = current_language(hwnd);
    config.sound = is_checked(hwnd, ID_SOUND);
    config.full_screen_reminders = is_checked(hwnd, ID_FULLSCREEN_REMINDERS);
    config.focus_mode_enabled = is_checked(hwnd, ID_FOCUS_MODE);
    config.focus_countdown_seconds = read_u64(
        hwnd,
        ID_FOCUS_SECONDS,
        config.focus_countdown_seconds,
        5,
        300,
    );
    config.overlay_style = read_overlay_style(hwnd, config.overlay_style);
    config.overlay_accent = read_overlay_accent(hwnd, config.overlay_accent);

    let (reply_tx, reply_rx) = mpsc::channel();
    let language = config.language;
    let text = settings_text(language);
    set_status(hwnd, text.sending_test);
    state
        .notifier_tx
        .send(NotifierCommand::ShowAndReport {
            kind,
            config,
            reply: reply_tx,
        })
        .context("cannot send test notification")?;

    match reply_rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok(())) => {
            set_status(hwnd, test_success_text(kind, language));
            Ok(())
        }
        Ok(Err(message)) => {
            set_status(hwnd, text.toast_failed);
            Err(anyhow!(message))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            set_status(hwnd, text.notifier_timeout);
            Err(anyhow!(text.notifier_timeout))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            set_status(hwnd, text.notifier_stopped);
            Err(anyhow!(text.notifier_stopped))
        }
    }
}

fn show_about(hwnd: HWND) -> Result<()> {
    let state = current_state()?;
    let language = current_language(hwnd);
    show_message(
        hwnd,
        about_title(language),
        &about_text(language, &state.config_path, &state.stats_path),
        false,
    );
    Ok(())
}

fn check_for_updates(hwnd: HWND) -> Result<()> {
    let language = current_language(hwnd);
    let text = settings_text(language);
    open_url(hwnd, core::update_url())?;
    set_status(hwnd, text.updates_opened);
    Ok(())
}

fn refresh_language_from_window(hwnd: HWND) {
    let language = current_language(hwnd);
    update_cached_language(language);
    apply_language(hwnd, language);
    crate::preview_tray_language(language);

    match current_state() {
        Ok(state) => {
            let mut config = core::load_config(&state.config_path);
            apply_dashboard_preview_from_window(hwnd, &mut config, language);
            refresh_dashboard(hwnd, &state.stats_path, &config);
            let _ = state.scheduler_tx.send(SchedulerCommand::Reload(config));
        }
        Err(error) => log::warn!("cannot refresh settings language: {error:#}"),
    }
}

fn apply_dashboard_preview_from_window(
    hwnd: HWND,
    config: &mut core::AppConfig,
    language: core::Language,
) {
    config.language = language;
    config.default_glass_ml = read_u64(hwnd, ID_DEFAULT_GLASS, config.default_glass_ml, 50, 2000);
    config.water_goal_auto = is_checked(hwnd, ID_WATER_GOAL_AUTO);
    config.water_weight_kg = read_u64(hwnd, ID_WATER_WEIGHT, config.water_weight_kg, 20, 250);
    config.water_gender = read_gender(hwnd, config.water_gender);
    config.water_unit = read_water_unit(hwnd, config.water_unit);
    config.water_goal_ml_override = read_u64(
        hwnd,
        ID_WATER_GOAL_OVERRIDE,
        config.water_goal_ml_override,
        500,
        6000,
    );
}

fn open_url(hwnd: HWND, url: &str) -> Result<()> {
    let operation = core::wide_null("open");
    let url = core::wide_null(url);
    let result = unsafe {
        ShellExecuteW(
            hwnd,
            PCWSTR(operation.as_ptr()),
            PCWSTR(url.as_ptr()),
            PCWSTR(std::ptr::null()),
            PCWSTR(std::ptr::null()),
            SW_SHOW,
        )
    };
    if result.0 as isize <= 32 {
        Err(anyhow!(
            "Không mở được trang cập nhật. ShellExecuteW={}",
            result.0
        ))
    } else {
        Ok(())
    }
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match std::panic::catch_unwind(|| settings_wnd_proc_inner(hwnd, msg, wparam, lparam)) {
        Ok(result) => result,
        Err(_) => {
            log::error!("settings window procedure panicked");
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

unsafe fn settings_wnd_proc_inner(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xffff) as i32;
            let notification = ((wparam.0 >> 16) & 0xffff) as u16;
            if id == ID_LANGUAGE && notification == CBN_SELCHANGE {
                refresh_language_from_window(hwnd);
                return LRESULT(0);
            }
            if id == ID_THEME && notification == CBN_SELCHANGE {
                apply_theme(hwnd);
                return LRESULT(0);
            }
            if id == ID_AUTOSTART && notification == BN_CLICKED {
                update_autostart_from_window(hwnd);
                return LRESULT(0);
            }
            if let Some(page) = page_from_nav_id(id) {
                apply_settings_page(hwnd, page);
                return LRESULT(0);
            }

            match id {
                ID_SAVE => match save_from_window(hwnd) {
                    Ok(()) => {
                        let text = settings_text(current_language(hwnd));
                        set_status(hwnd, text.saved);
                        show_message(hwnd, core::APP_NAME, text.saved, false);
                    }
                    Err(error) => {
                        let text = settings_text(current_language(hwnd));
                        log::warn!("settings save failed: {error:#}");
                        set_status(hwnd, text.save_failed);
                        show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
                    }
                },
                ID_TEST_WATER => {
                    if let Err(error) = send_test(ReminderKind::Water, hwnd) {
                        show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
                    }
                }
                ID_TEST_EYES => {
                    if let Err(error) = send_test(ReminderKind::Eyes, hwnd) {
                        show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
                    }
                }
                ID_TEST_STAND => {
                    if let Err(error) = send_test(ReminderKind::Stand, hwnd) {
                        show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
                    }
                }
                ID_TEST_WORK => {
                    if let Err(error) = send_test(ReminderKind::Work, hwnd) {
                        show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
                    }
                }
                ID_LOG_WATER => {
                    if let Err(error) = log_water_from_window(hwnd) {
                        show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
                    }
                }
                ID_ABOUT => {
                    if let Err(error) = show_about(hwnd) {
                        show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
                    }
                }
                ID_CHECK_UPDATES => {
                    if let Err(error) = check_for_updates(hwnd) {
                        show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
                    }
                }
                ID_HIDE => {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    core::trim_memory();
                }
                _ => {}
            }
            return LRESULT(0);
        }
        WM_CLOSE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            core::trim_memory();
            return LRESULT(0);
        }
        WM_UAHDRAWMENU | WM_UAHDRAWMENUITEM => {
            theme::draw_menu_bar(hwnd, msg, lparam, current_theme(hwnd));
            return LRESULT(0);
        }
        WM_NCACTIVATE | WM_NCPAINT => {
            let result = DefWindowProcW(hwnd, msg, wparam, lparam);
            theme::draw_menu_bar(hwnd, msg, lparam, current_theme(hwnd));
            return result;
        }
        WM_ERASEBKGND => {
            return theme::erase_background(hwnd, wparam, current_theme(hwnd));
        }
        WM_MEASUREITEM => {
            if let Some(result) = theme::measure_owner_menu(hwnd, lparam) {
                return result;
            }
        }
        WM_DRAWITEM => {
            if let Some(result) = theme::draw_owner_button(hwnd, lparam, current_theme(hwnd)) {
                return result;
            }
            if let Some(result) = theme::draw_owner_menu(hwnd, lparam, current_theme(hwnd)) {
                return result;
            }
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            if let Some(result) =
                theme::control_color(hwnd, wparam, lparam, false, current_theme(hwnd))
            {
                return result;
            }
        }
        WM_CTLCOLOREDIT => {
            if let Some(result) =
                theme::control_color(hwnd, wparam, lparam, true, current_theme(hwnd))
            {
                return result;
            }
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            apply_theme(hwnd);
            normalize_settings_window(hwnd);
            return LRESULT(0);
        }
        WM_DPICHANGED => {
            normalize_settings_window(hwnd);
            return LRESULT(0);
        }
        WM_EXITSIZEMOVE => {
            normalize_settings_window(hwnd);
            return LRESULT(0);
        }
        _ => {}
    }

    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn apply_theme(hwnd: HWND) {
    let theme = current_theme(hwnd);
    update_cached_theme(theme);
    theme::apply_theme(hwnd, theme);
}

fn update_autostart_from_window(hwnd: HWND) {
    let enabled = is_checked(hwnd, ID_AUTOSTART);
    match core::set_autostart(enabled) {
        Ok(()) => crate::preview_tray_autostart(enabled),
        Err(error) => {
            log::warn!("autostart update failed: {error:#}");
            let actual = core::autostart_is_enabled().unwrap_or(false);
            set_checked(hwnd, ID_AUTOSTART, actual);
            crate::preview_tray_autostart(actual);
            show_message(hwnd, core::APP_NAME, &format!("{error:#}"), true);
        }
    }
}

fn current_theme(hwnd: HWND) -> core::ThemeMode {
    let fallback = current_state().map(|state| state.theme).unwrap_or_default();
    read_theme(hwnd, fallback)
}

fn update_cached_appearance(theme: core::ThemeMode, language: core::Language) {
    if let Some(lock) = SETTINGS.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(state) = guard.as_mut() {
                state.theme = theme;
                state.language = language;
            }
        }
    }
}

fn update_cached_theme(theme: core::ThemeMode) {
    if let Some(lock) = SETTINGS.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(state) = guard.as_mut() {
                state.theme = theme;
            }
        }
    }
}

fn update_cached_language(language: core::Language) {
    if let Some(lock) = SETTINGS.get() {
        if let Ok(mut guard) = lock.lock() {
            if let Some(state) = guard.as_mut() {
                state.language = language;
            }
        }
    }
}

fn current_state() -> Result<SettingsState> {
    let lock = SETTINGS
        .get()
        .ok_or_else(|| anyhow!("settings window is not initialized"))?;
    let guard = lock_settings(lock)?;
    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| anyhow!("settings state is empty"))
}

fn lock_settings(
    lock: &Mutex<Option<SettingsState>>,
) -> Result<MutexGuard<'_, Option<SettingsState>>> {
    lock.lock()
        .map_err(|_| anyhow!("settings state lock poisoned"))
}

pub(super) fn settings_font_handle(parent: HWND) -> isize {
    let dpi = effective_ui_dpi(parent);
    if dpi <= STANDARD_DPI {
        return unsafe { GetStockObject(DEFAULT_GUI_FONT) }.0;
    }

    let lock = SETTINGS_FONTS.get_or_init(|| Mutex::new(Vec::new()));
    let mut guard = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(font) = guard.iter().find(|font| font.dpi == dpi) {
        return font.handle;
    }

    let face = core::wide_null("Segoe UI");
    let height = -((9 * dpi as i32 + 36) / 72);
    let font = unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            PCWSTR(face.as_ptr()),
        )
    };
    if font.0 == 0 {
        return unsafe { GetStockObject(DEFAULT_GUI_FONT) }.0;
    }

    guard.push(SettingsFont {
        dpi,
        handle: font.0,
    });
    font.0
}

fn apply_scaled_fonts(parent: HWND) {
    for id in CONTROL_IDS {
        let hwnd = unsafe { GetDlgItem(parent, *id) };
        if hwnd.0 != 0 {
            set_control_font(parent, hwnd);
        }
    }
}

fn set_control_font(parent: HWND, hwnd: HWND) {
    let font = settings_font_handle(parent);
    if font != 0 {
        unsafe {
            let _ = SendMessageW(hwnd, WM_SETFONT, WPARAM(font as usize), LPARAM(1));
        }
    }
}

fn set_edit_u64(parent: HWND, id: i32, value: u64) {
    set_control_text(parent, id, &value.to_string());
}

fn set_edit_hhmm(parent: HWND, id: i32, value: u16) {
    let value = value.min(1439);
    let text = format!("{:02}:{:02}", value / 60, value % 60);
    set_control_text(parent, id, &text);
}

fn set_combo_index(parent: HWND, id: i32, index: usize) {
    let hwnd = unsafe { GetDlgItem(parent, id) };
    if hwnd.0 == 0 {
        return;
    }
    unsafe {
        let _ = SendMessageW(hwnd, CB_SETCURSEL, WPARAM(index), LPARAM(0));
    }
}

fn set_status(parent: HWND, value: &str) {
    set_control_text(parent, ID_STATUS, value);
}

fn refresh_dashboard(parent: HWND, stats_path: &Path, config: &core::AppConfig) {
    let today = core::today_stats(stats_path);
    let stats = core::load_stats(stats_path);
    let streak = core::current_streak(&stats);
    let goal = core::water_goal_ml(config);
    let percent = core::water_progress_percent(today.water_ml, goal);
    let remaining = goal.saturating_sub(today.water_ml);
    let dashboard = match config.language {
        core::Language::Vietnamese => format!(
            "Bạn đã uống: {} / {} ({}%)\r\nCòn lại hôm nay: {}\r\nSố ly đã ghi: {} {}\r\nNghỉ mắt: {} lần | Vận động: {} lần\r\nChuỗi: {} ngày liên tiếp",
            core::format_water_volume(today.water_ml, config.water_unit),
            core::format_water_volume(goal, config.water_unit),
            percent,
            core::format_water_volume(remaining, config.water_unit),
            today.water_glasses,
            glass_label(config.language),
            today.eye_rest_completed,
            today.movement_completed,
            streak
        ),
        core::Language::English => format!(
            "Drank: {} / {} ({}%)\r\nLeft today: {}\r\nGlasses logged: {} {}\r\nEye rest: {} | Movement: {}\r\nStreak: {} days",
            core::format_water_volume(today.water_ml, config.water_unit),
            core::format_water_volume(goal, config.water_unit),
            percent,
            core::format_water_volume(remaining, config.water_unit),
            today.water_glasses,
            glass_label(config.language),
            today.eye_rest_completed,
            today.movement_completed,
            streak
        ),
    };
    set_control_text(parent, ID_DASHBOARD, &dashboard);
    set_control_text(parent, ID_WATER_CHART, &water_chart_text(&stats, config));
}

fn set_window_text(hwnd: HWND, value: &str) {
    let text = core::wide_null(value);
    unsafe {
        let _ = SetWindowTextW(hwnd, PCWSTR(text.as_ptr()));
    }
}

fn set_control_text(parent: HWND, id: i32, value: &str) {
    let hwnd = unsafe { GetDlgItem(parent, id) };
    if hwnd.0 == 0 {
        return;
    }
    set_window_text(hwnd, value);
}

fn current_language(parent: HWND) -> core::Language {
    let fallback = current_state()
        .map(|state| state.language)
        .unwrap_or_default();
    read_language(parent, fallback)
}

fn read_u64(parent: HWND, id: i32, fallback: u64, min: u64, max: u64) -> u64 {
    let mut buffer = [0u16; 32];
    let len = unsafe { GetDlgItemTextW(parent, id, &mut buffer) } as usize;
    if len == 0 {
        return fallback.clamp(min, max);
    }

    String::from_utf16_lossy(&buffer[..len])
        .trim()
        .parse::<u64>()
        .map(|value| value.clamp(min, max))
        .unwrap_or_else(|_| fallback.clamp(min, max))
}

fn read_stepped_u64(parent: HWND, id: i32, fallback: u64, min: u64, max: u64, step: u64) -> u64 {
    let value = read_u64(parent, id, fallback, min, max);
    let step = step.max(1);
    let stepped = ((value + (step / 2)) / step) * step;
    stepped.clamp(min, max)
}

fn read_hhmm(parent: HWND, id: i32, fallback: u16) -> u16 {
    let mut buffer = [0u16; 32];
    let len = unsafe { GetDlgItemTextW(parent, id, &mut buffer) } as usize;
    if len == 0 {
        return fallback.min(1439);
    }

    let text = String::from_utf16_lossy(&buffer[..len]);
    let text = text.trim();
    if let Some((hour, minute)) = text.split_once(':') {
        let parsed_hour = hour.trim().parse::<u16>();
        let parsed_minute = minute.trim().parse::<u16>();
        if let (Ok(hour), Ok(minute)) = (parsed_hour, parsed_minute) {
            if hour < 24 && minute < 60 {
                return hour * 60 + minute;
            }
        }
        return fallback.min(1439);
    }

    match text.parse::<u16>() {
        Ok(value) => {
            let hour = value / 100;
            let minute = value % 100;
            if hour < 24 && minute < 60 {
                hour * 60 + minute
            } else {
                fallback.min(1439)
            }
        }
        Err(_) => fallback.min(1439),
    }
}

fn read_theme(parent: HWND, fallback: core::ThemeMode) -> core::ThemeMode {
    match combo_index(parent, ID_THEME) {
        Some(0) => core::ThemeMode::System,
        Some(1) => core::ThemeMode::Light,
        Some(2) => core::ThemeMode::Dark,
        _ => fallback,
    }
}

fn read_language(parent: HWND, fallback: core::Language) -> core::Language {
    match combo_index(parent, ID_LANGUAGE) {
        Some(0) => core::Language::Vietnamese,
        Some(1) => core::Language::English,
        _ => fallback,
    }
}

fn read_gender(parent: HWND, fallback: core::Gender) -> core::Gender {
    match combo_index(parent, ID_WATER_GENDER) {
        Some(0) => core::Gender::Other,
        Some(1) => core::Gender::Male,
        Some(2) => core::Gender::Female,
        _ => fallback,
    }
}

fn read_water_unit(parent: HWND, fallback: core::WaterUnit) -> core::WaterUnit {
    match combo_index(parent, ID_WATER_UNIT) {
        Some(0) => core::WaterUnit::Milliliter,
        Some(1) => core::WaterUnit::Ounce,
        _ => fallback,
    }
}

fn read_overlay_style(parent: HWND, fallback: core::OverlayStyle) -> core::OverlayStyle {
    match combo_index(parent, ID_OVERLAY_STYLE) {
        Some(0) => core::OverlayStyle::Modern,
        Some(1) => core::OverlayStyle::Minimal,
        Some(2) => core::OverlayStyle::Bold,
        _ => fallback,
    }
}

fn read_overlay_accent(parent: HWND, fallback: core::OverlayAccent) -> core::OverlayAccent {
    match combo_index(parent, ID_OVERLAY_ACCENT) {
        Some(0) => core::OverlayAccent::Blue,
        Some(1) => core::OverlayAccent::Green,
        Some(2) => core::OverlayAccent::Amber,
        _ => fallback,
    }
}

fn read_movement_snooze(parent: HWND, fallback: u64) -> u64 {
    match combo_index(parent, ID_MOVEMENT_SNOOZE) {
        Some(0) => 5,
        Some(1) => 10,
        Some(2) => 15,
        _ => fallback.clamp(5, 15),
    }
}

fn combo_index(parent: HWND, id: i32) -> Option<usize> {
    let hwnd = unsafe { GetDlgItem(parent, id) };
    if hwnd.0 == 0 {
        return None;
    }

    let result = unsafe { SendMessageW(hwnd, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 };
    if result < 0 {
        None
    } else {
        Some(result as usize)
    }
}

fn theme_index(theme: core::ThemeMode) -> usize {
    match theme {
        core::ThemeMode::System => 0,
        core::ThemeMode::Light => 1,
        core::ThemeMode::Dark => 2,
    }
}

fn language_index(language: core::Language) -> usize {
    match language {
        core::Language::Vietnamese => 0,
        core::Language::English => 1,
    }
}

fn gender_index(gender: core::Gender) -> usize {
    match gender {
        core::Gender::Other => 0,
        core::Gender::Male => 1,
        core::Gender::Female => 2,
    }
}

fn water_unit_index(unit: core::WaterUnit) -> usize {
    match unit {
        core::WaterUnit::Milliliter => 0,
        core::WaterUnit::Ounce => 1,
    }
}

fn overlay_style_index(style: core::OverlayStyle) -> usize {
    match style {
        core::OverlayStyle::Modern => 0,
        core::OverlayStyle::Minimal => 1,
        core::OverlayStyle::Bold => 2,
    }
}

fn overlay_accent_index(accent: core::OverlayAccent) -> usize {
    match accent {
        core::OverlayAccent::Blue => 0,
        core::OverlayAccent::Green => 1,
        core::OverlayAccent::Amber => 2,
    }
}

fn movement_snooze_index(value: u64) -> usize {
    match value {
        5 => 0,
        15 => 2,
        _ => 1,
    }
}

fn set_checked(parent: HWND, id: i32, checked: bool) {
    let hwnd = unsafe { GetDlgItem(parent, id) };
    if hwnd.0 == 0 {
        return;
    }
    unsafe {
        let _ = SendMessageW(
            hwnd,
            BM_SETCHECK,
            WPARAM(if checked { 1 } else { 0 }),
            LPARAM(0),
        );
    }
}

fn is_checked(parent: HWND, id: i32) -> bool {
    let hwnd = unsafe { GetDlgItem(parent, id) };
    hwnd.0 != 0 && unsafe { SendMessageW(hwnd, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1 }
}

fn show_message(hwnd: HWND, title: &str, body: &str, error: bool) {
    let title = core::wide_null(title);
    let body = core::wide_null(body);
    let flags = if error {
        MB_OK | MB_ICONERROR
    } else {
        MB_OK | MB_ICONINFORMATION
    };
    unsafe {
        let _ = MessageBoxW(hwnd, PCWSTR(body.as_ptr()), PCWSTR(title.as_ptr()), flags);
    }
}

fn style_bits(value: i32) -> WINDOW_STYLE {
    WINDOW_STYLE(value as u32)
}
