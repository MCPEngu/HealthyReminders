use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};

use windows::{
    Win32::{
        Foundation::{BOOL, COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
            Gdi::{
                CreateSolidBrush, DT_CENTER, DT_HIDEPREFIX, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
                DrawTextW, FillRect, FrameRect, GetWindowDC, HBRUSH, HDC, HGDIOBJ, InvalidateRect,
                MapWindowPoints, OPAQUE, OffsetRect, ReleaseDC, SelectObject, SetBkColor,
                SetBkMode, SetTextColor, TRANSPARENT, UpdateWindow,
            },
        },
        UI::{
            Controls::{
                DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_DISABLED, ODS_GRAYED, ODS_HOTLIGHT,
                ODS_NOACCEL, ODS_SELECTED, ODT_BUTTON, ODT_MENU, SetWindowTheme,
            },
            WindowsAndMessaging::{
                DrawMenuBar, GetClientRect, GetDlgCtrlID, GetDlgItem, GetMenu, GetMenuBarInfo,
                GetMenuItemInfoW, GetWindowRect, GetWindowTextW, HMENU, MENUBARINFO, MENUINFO,
                MENUITEMINFOW, MIIM_STRING, MIM_APPLYTOSUBMENUS, MIM_BACKGROUND, MIM_STYLE,
                MNS_NOCHECK, OBJID_MENU, SetMenuInfo, WM_NCACTIVATE, WM_NCPAINT,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};

use crate::core;

use super::ids::{
    CONTROL_IDS, ID_DASHBOARD, ID_SECTION_APP, ID_SECTION_DISPLAY, ID_SECTION_REMINDERS,
    ID_SECTION_SCHEDULE, ID_SECTION_TODAY, ID_SECTION_WATER, ID_STATUS, ID_WATER_CHART,
};

static THEME_BRUSHES: OnceLock<ThemeBrushes> = OnceLock::new();
static APPLYING_THEME: AtomicBool = AtomicBool::new(false);

struct ThemeBrushes {
    light_background: isize,
    light_edit: isize,
    light_button: isize,
    light_button_pressed: isize,
    light_border: isize,
    light_menu_hot: isize,
    dark_background: isize,
    dark_edit: isize,
    dark_button: isize,
    dark_button_pressed: isize,
    dark_border: isize,
    dark_menu_hot: isize,
}

#[repr(C)]
#[allow(dead_code)]
struct UahMenuItemMetrics0 {
    cx: u32,
    cy: u32,
}

#[repr(C)]
#[allow(dead_code)]
struct UahMenuItemMetrics {
    rgsize_bar: [UahMenuItemMetrics0; 2],
    rgsize_popup: [UahMenuItemMetrics0; 4],
}

#[repr(C)]
#[allow(dead_code)]
struct UahMenuPopupMetrics {
    rgcx: [u32; 4],
    update_max_widths: u32,
}

#[repr(C)]
#[allow(dead_code)]
struct UahMenu {
    hmenu: HMENU,
    hdc: HDC,
    flags: u32,
}

#[repr(C)]
#[allow(dead_code)]
struct UahMenuItem {
    position: u32,
    item_metrics: UahMenuItemMetrics,
    popup_metrics: UahMenuPopupMetrics,
}

#[repr(C)]
#[allow(dead_code)]
struct UahDrawMenuItem {
    draw_item: DRAWITEMSTRUCT,
    menu: UahMenu,
    item: UahMenuItem,
}

struct ThemeApplyGuard;

impl Drop for ThemeApplyGuard {
    fn drop(&mut self) {
        APPLYING_THEME.store(false, Ordering::Release);
    }
}

pub(super) fn is_dark(theme: core::ThemeMode) -> bool {
    core::dark_mode_enabled(theme)
}

pub(super) fn apply_theme(hwnd: HWND, theme: core::ThemeMode) {
    let Some(_guard) = begin_theme_apply() else {
        return;
    };
    let dark = is_dark(theme);
    set_dark_title_bar(hwnd, dark);
    apply_control_themes(hwnd, dark);
    let menu = unsafe { GetMenu(hwnd) };
    if menu.0 != 0 {
        apply_menu_theme(menu, theme);
    }
    unsafe {
        let _ = DrawMenuBar(hwnd);
        let _ = InvalidateRect(hwnd, None, BOOL(1));
        let _ = UpdateWindow(hwnd);
    }
}

pub(super) fn apply_menu_theme(menu: HMENU, theme: core::ThemeMode) {
    let dark = is_dark(theme);
    let menu_info = MENUINFO {
        cbSize: std::mem::size_of::<MENUINFO>() as u32,
        fMask: MIM_BACKGROUND | MIM_APPLYTOSUBMENUS | MIM_STYLE,
        dwStyle: MNS_NOCHECK,
        hbrBack: theme_brush(dark, false),
        ..Default::default()
    };
    unsafe {
        let _ = SetMenuInfo(menu, &menu_info);
    }
}

pub(super) fn draw_menu_bar(hwnd: HWND, msg: u32, lparam: LPARAM, theme: core::ThemeMode) {
    let dark = is_dark(theme);
    match msg {
        WM_NCACTIVATE | WM_NCPAINT => draw_menu_bar_separator(hwnd, dark),
        super::WM_UAHDRAWMENU => draw_menu_background(hwnd, lparam, dark),
        super::WM_UAHDRAWMENUITEM => draw_menu_item(hwnd, lparam, dark),
        _ => {}
    }
}

pub(super) fn erase_background(hwnd: HWND, wparam: WPARAM, theme: core::ThemeMode) -> LRESULT {
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetClientRect(hwnd, &mut rect) }.is_ok() {
        let dark = is_dark(theme);
        unsafe {
            let _ = FillRect(HDC(wparam.0 as isize), &rect, theme_brush(dark, false));
        }
        return LRESULT(1);
    }

    LRESULT(0)
}

pub(super) fn draw_owner_button(
    parent: HWND,
    lparam: LPARAM,
    theme: core::ThemeMode,
) -> Option<LRESULT> {
    let draw_item = unsafe { (lparam.0 as *const DRAWITEMSTRUCT).as_ref()? };
    if draw_item.CtlType != ODT_BUTTON {
        return None;
    }

    let dark = is_dark(theme);
    let state = draw_item.itemState.0;
    let pressed = state & ODS_SELECTED.0 != 0;
    let disabled = state & (ODS_DISABLED.0 | ODS_GRAYED.0) != 0;
    let mut rect = draw_item.rcItem;

    unsafe {
        let _ = FillRect(draw_item.hDC, &rect, button_brush(dark, pressed));
        let _ = FrameRect(draw_item.hDC, &rect, button_border_brush(dark));
    }

    let mut label = [0u16; 128];
    let len = unsafe { GetWindowTextW(draw_item.hwndItem, &mut label) }
        .max(0)
        .min(label.len() as i32) as usize;
    if len > 0 {
        rect.left += 6;
        rect.right -= 6;
        if pressed {
            rect.left += 1;
            rect.top += 1;
        }

        unsafe {
            let _ = SetTextColor(
                draw_item.hDC,
                if disabled {
                    if dark {
                        rgb(107, 114, 128)
                    } else {
                        rgb(156, 163, 175)
                    }
                } else if dark {
                    rgb(243, 244, 246)
                } else {
                    rgb(17, 24, 39)
                },
            );
            let _ = SetBkMode(draw_item.hDC, TRANSPARENT);
            let previous_font = select_ui_font(parent, draw_item.hDC);
            let _ = DrawTextW(
                draw_item.hDC,
                &mut label[..len],
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
            restore_ui_font(draw_item.hDC, previous_font);
        }
    }

    Some(LRESULT(1))
}

pub(super) fn measure_owner_menu(parent: HWND, lparam: LPARAM) -> Option<LRESULT> {
    let measure_item = unsafe { (lparam.0 as *mut MEASUREITEMSTRUCT).as_mut()? };
    if measure_item.CtlType != ODT_MENU {
        return None;
    }

    if measure_item.itemData == 0 {
        measure_item.itemWidth = 1;
        measure_item.itemHeight = super::scale_value(parent, 9) as u32;
        return Some(LRESULT(1));
    }

    let label_len = owner_menu_label_len(measure_item.itemData);
    let text_width = (label_len as u32).saturating_mul(super::scale_value(parent, 9) as u32);
    measure_item.itemWidth = text_width
        .saturating_add(super::scale_value(parent, 48) as u32)
        .max(super::scale_value(parent, 150) as u32);
    measure_item.itemHeight = super::scale_value(parent, 30) as u32;
    Some(LRESULT(1))
}

pub(super) fn draw_owner_menu(
    parent: HWND,
    lparam: LPARAM,
    theme: core::ThemeMode,
) -> Option<LRESULT> {
    let draw_item = unsafe { (lparam.0 as *const DRAWITEMSTRUCT).as_ref()? };
    if draw_item.CtlType != ODT_MENU {
        return None;
    }

    let dark = is_dark(theme);
    let selected = draw_item.itemState.0 & (ODS_HOTLIGHT.0 | ODS_SELECTED.0) != 0;
    let disabled = draw_item.itemState.0 & (ODS_DISABLED.0 | ODS_GRAYED.0) != 0;
    let brush = if selected {
        menu_hot_brush(dark)
    } else {
        theme_brush(dark, false)
    };
    let rect = draw_item.rcItem;

    unsafe {
        let _ = FillRect(draw_item.hDC, &rect, brush);
    }

    if draw_item.itemData == 0 {
        draw_owner_menu_separator(draw_item.hDC, rect, dark);
        return Some(LRESULT(1));
    }

    let mut label = owner_menu_label(draw_item.itemData);
    if label.is_empty() {
        return Some(LRESULT(1));
    }

    let mut text_rect = rect;
    text_rect.left += super::scale_value(parent, 24);
    text_rect.right -= super::scale_value(parent, 16);

    unsafe {
        let _ = SetTextColor(
            draw_item.hDC,
            if disabled {
                if dark {
                    rgb(107, 114, 128)
                } else {
                    rgb(148, 163, 184)
                }
            } else if dark {
                rgb(243, 244, 246)
            } else {
                rgb(17, 24, 39)
            },
        );
        let _ = SetBkMode(draw_item.hDC, TRANSPARENT);
        let previous_font = select_ui_font(parent, draw_item.hDC);
        let _ = DrawTextW(
            draw_item.hDC,
            &mut label,
            &mut text_rect,
            DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        restore_ui_font(draw_item.hDC, previous_font);
    }

    Some(LRESULT(1))
}

pub(super) fn control_color(
    hwnd: HWND,
    wparam: WPARAM,
    lparam: LPARAM,
    edit: bool,
    theme: core::ThemeMode,
) -> Option<LRESULT> {
    let dark = is_dark(theme);
    let hdc = HDC(wparam.0 as isize);
    let child = HWND(lparam.0);
    let status = unsafe { GetDlgItem(hwnd, ID_STATUS) };
    let dashboard = unsafe { GetDlgItem(hwnd, ID_DASHBOARD) };
    let chart = unsafe { GetDlgItem(hwnd, ID_WATER_CHART) };
    let child_id = unsafe { GetDlgCtrlID(child) };
    let muted_control = child.0 == status.0 || child.0 == dashboard.0 || child.0 == chart.0;
    let text_color = if dark {
        if is_section_control(child_id) {
            rgb(125, 211, 252)
        } else if muted_control {
            rgb(178, 186, 198)
        } else {
            rgb(239, 242, 246)
        }
    } else if is_section_control(child_id) {
        rgb(29, 78, 216)
    } else if muted_control {
        rgb(75, 85, 99)
    } else {
        rgb(17, 24, 39)
    };
    let background = if dark {
        if edit {
            rgb(18, 20, 24)
        } else {
            rgb(31, 34, 40)
        }
    } else {
        rgb(255, 255, 255)
    };

    unsafe {
        let _ = SetTextColor(hdc, text_color);
        let _ = SetBkColor(hdc, background);
        let _ = SetBkMode(hdc, OPAQUE);
    }
    Some(LRESULT(theme_brush(dark, edit).0))
}

fn begin_theme_apply() -> Option<ThemeApplyGuard> {
    APPLYING_THEME
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .ok()
        .map(|_| ThemeApplyGuard)
}

fn set_dark_title_bar(hwnd: HWND, dark: bool) {
    let enabled: i32 = if dark { 1 } else { 0 };
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &enabled as *const _ as *const _,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

fn apply_control_themes(parent: HWND, dark: bool) {
    let theme = core::wide_null(if dark {
        "DarkMode_Explorer"
    } else {
        "Explorer"
    });
    unsafe {
        let _ = SetWindowTheme(parent, PCWSTR(theme.as_ptr()), PCWSTR(std::ptr::null()));
        for id in CONTROL_IDS {
            let hwnd = GetDlgItem(parent, *id);
            if hwnd.0 != 0 {
                let _ = SetWindowTheme(hwnd, PCWSTR(theme.as_ptr()), PCWSTR(std::ptr::null()));
                let _ = InvalidateRect(hwnd, None, BOOL(1));
                let _ = UpdateWindow(hwnd);
            }
        }
    }
}

fn draw_menu_bar_separator(hwnd: HWND, dark: bool) {
    let mut client_rect = RECT::default();
    if unsafe { GetClientRect(hwnd, &mut client_rect) }.is_err() {
        return;
    }

    let mut points = [
        POINT {
            x: client_rect.left,
            y: client_rect.top,
        },
        POINT {
            x: client_rect.right,
            y: client_rect.bottom,
        },
    ];
    unsafe {
        let _ = MapWindowPoints(hwnd, HWND(0), &mut points);
    }
    client_rect = RECT {
        left: points[0].x,
        top: points[0].y,
        right: points[1].x,
        bottom: points[1].y,
    };

    let mut window_rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window_rect) }.is_err() {
        return;
    }

    unsafe {
        let _ = OffsetRect(&mut client_rect, -window_rect.left, -window_rect.top);
    }

    let mut separator_rect = client_rect;
    separator_rect.bottom = separator_rect.top;
    separator_rect.top -= 1;

    unsafe {
        let hdc = GetWindowDC(hwnd);
        if hdc.0 != 0 {
            let _ = FillRect(hdc, &separator_rect, theme_brush(dark, false));
            let _ = ReleaseDC(hwnd, hdc);
        }
    }
}

fn draw_menu_background(hwnd: HWND, lparam: LPARAM, dark: bool) {
    let Some(menu) = (unsafe { (lparam.0 as *const UahMenu).as_ref() }) else {
        return;
    };
    let Some(rect) = menu_bar_rect(hwnd) else {
        return;
    };
    unsafe {
        let _ = FillRect(menu.hdc, &rect, theme_brush(dark, false));
    }
}

fn draw_menu_item(parent: HWND, lparam: LPARAM, dark: bool) {
    let Some(draw_menu_item) = (unsafe { (lparam.0 as *mut UahDrawMenuItem).as_mut() }) else {
        return;
    };

    let state = draw_menu_item.draw_item.itemState.0;
    let selected = state & (ODS_HOTLIGHT.0 | ODS_SELECTED.0) != 0;
    let disabled = state & (ODS_DISABLED.0 | ODS_GRAYED.0) != 0;
    let mut text_flags = DT_CENTER | DT_SINGLELINE | DT_VCENTER;
    if state & ODS_NOACCEL.0 != 0 {
        text_flags |= DT_HIDEPREFIX;
    }

    let mut label = [0u16; 256];
    let mut info = MENUITEMINFOW {
        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
        fMask: MIIM_STRING,
        dwTypeData: PWSTR(label.as_mut_ptr()),
        cch: label.len().saturating_sub(1) as u32,
        ..Default::default()
    };
    let _ = unsafe {
        GetMenuItemInfoW(
            draw_menu_item.menu.hmenu,
            draw_menu_item.item.position,
            BOOL(1),
            &mut info,
        )
    };

    let len = (info.cch as usize).min(label.len());
    let brush = if selected {
        menu_hot_brush(dark)
    } else {
        theme_brush(dark, false)
    };
    let mut text_rect = draw_menu_item.draw_item.rcItem;

    unsafe {
        let _ = FillRect(draw_menu_item.menu.hdc, &text_rect, brush);
        let _ = SetTextColor(
            draw_menu_item.menu.hdc,
            if disabled {
                if dark {
                    rgb(107, 114, 128)
                } else {
                    rgb(148, 163, 184)
                }
            } else if dark {
                rgb(243, 244, 246)
            } else {
                rgb(17, 24, 39)
            },
        );
        let _ = SetBkMode(draw_menu_item.menu.hdc, TRANSPARENT);
        if len > 0 {
            let previous_font = select_ui_font(parent, draw_menu_item.menu.hdc);
            let _ = DrawTextW(
                draw_menu_item.menu.hdc,
                &mut label[..len],
                &mut text_rect,
                text_flags,
            );
            restore_ui_font(draw_menu_item.menu.hdc, previous_font);
        }
    }
}

fn menu_bar_rect(hwnd: HWND) -> Option<RECT> {
    let mut menu_info = MENUBARINFO {
        cbSize: std::mem::size_of::<MENUBARINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMenuBarInfo(hwnd, OBJID_MENU, 0, &mut menu_info) }.is_err() {
        return None;
    }

    let mut window_rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut window_rect) }.is_err() {
        return None;
    }

    let mut rect = menu_info.rcBar;
    unsafe {
        let _ = OffsetRect(&mut rect, -window_rect.left, -window_rect.top);
    }
    rect.top -= 1;
    Some(rect)
}

fn draw_owner_menu_separator(hdc: HDC, mut rect: RECT, dark: bool) {
    unsafe {
        let _ = FillRect(hdc, &rect, theme_brush(dark, false));
    }

    let middle = rect.top + ((rect.bottom - rect.top) / 2);
    rect.left += 16;
    rect.right -= 16;
    rect.top = middle;
    rect.bottom = middle + 1;

    unsafe {
        let _ = FillRect(hdc, &rect, button_border_brush(dark));
    }
}

fn select_ui_font(parent: HWND, hdc: HDC) -> Option<HGDIOBJ> {
    let font = super::settings_font_handle(parent);
    if font == 0 {
        return None;
    }

    let previous = unsafe { SelectObject(hdc, HGDIOBJ(font)) };
    if previous.0 == 0 {
        None
    } else {
        Some(previous)
    }
}

fn restore_ui_font(hdc: HDC, previous: Option<HGDIOBJ>) {
    if let Some(font) = previous {
        unsafe {
            let _ = SelectObject(hdc, font);
        }
    }
}

fn owner_menu_label(item_data: usize) -> Vec<u16> {
    let ptr = item_data as *const u16;
    if ptr.is_null() {
        return Vec::new();
    }

    let len = owner_menu_label_len(item_data);
    if len == 0 {
        return Vec::new();
    }

    unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
}

fn owner_menu_label_len(item_data: usize) -> usize {
    let ptr = item_data as *const u16;
    if ptr.is_null() {
        return 0;
    }

    for len in 0..256 {
        if unsafe { *ptr.add(len) } == 0 {
            return len;
        }
    }
    256
}

fn is_section_control(id: i32) -> bool {
    matches!(
        id,
        ID_SECTION_TODAY
            | ID_SECTION_REMINDERS
            | ID_SECTION_WATER
            | ID_SECTION_SCHEDULE
            | ID_SECTION_DISPLAY
            | ID_SECTION_APP
    )
}

fn theme_brush(dark: bool, edit: bool) -> HBRUSH {
    let brushes = theme_brushes();
    let brush = match (dark, edit) {
        (true, true) => brushes.dark_edit,
        (true, false) => brushes.dark_background,
        (false, true) => brushes.light_edit,
        (false, false) => brushes.light_background,
    };
    HBRUSH(brush)
}

fn button_brush(dark: bool, pressed: bool) -> HBRUSH {
    let brushes = theme_brushes();
    let brush = match (dark, pressed) {
        (true, true) => brushes.dark_button_pressed,
        (true, false) => brushes.dark_button,
        (false, true) => brushes.light_button_pressed,
        (false, false) => brushes.light_button,
    };
    HBRUSH(brush)
}

fn button_border_brush(dark: bool) -> HBRUSH {
    let brushes = theme_brushes();
    HBRUSH(if dark {
        brushes.dark_border
    } else {
        brushes.light_border
    })
}

fn menu_hot_brush(dark: bool) -> HBRUSH {
    let brushes = theme_brushes();
    HBRUSH(if dark {
        brushes.dark_menu_hot
    } else {
        brushes.light_menu_hot
    })
}

fn theme_brushes() -> &'static ThemeBrushes {
    THEME_BRUSHES.get_or_init(|| ThemeBrushes {
        light_background: unsafe { CreateSolidBrush(rgb(255, 255, 255)) }.0,
        light_edit: unsafe { CreateSolidBrush(rgb(255, 255, 255)) }.0,
        light_button: unsafe { CreateSolidBrush(rgb(248, 250, 252)) }.0,
        light_button_pressed: unsafe { CreateSolidBrush(rgb(226, 232, 240)) }.0,
        light_border: unsafe { CreateSolidBrush(rgb(148, 163, 184)) }.0,
        light_menu_hot: unsafe { CreateSolidBrush(rgb(229, 241, 251)) }.0,
        dark_background: unsafe { CreateSolidBrush(rgb(31, 34, 40)) }.0,
        dark_edit: unsafe { CreateSolidBrush(rgb(18, 20, 24)) }.0,
        dark_button: unsafe { CreateSolidBrush(rgb(39, 44, 52)) }.0,
        dark_button_pressed: unsafe { CreateSolidBrush(rgb(55, 65, 81)) }.0,
        dark_border: unsafe { CreateSolidBrush(rgb(100, 116, 139)) }.0,
        dark_menu_hot: unsafe { CreateSolidBrush(rgb(51, 56, 66)) }.0,
    })
}

fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    COLORREF(red as u32 | ((green as u32) << 8) | ((blue as u32) << 16))
}
