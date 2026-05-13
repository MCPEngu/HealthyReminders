use std::{cell::RefCell, path::Path};

use anyhow::{Context, Result};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem},
};

use crate::core;

#[derive(Clone)]
pub(crate) struct TrayMenuItems {
    autostart: CheckMenuItem,
    settings: MenuItem,
    quit: MenuItem,
}

struct TrayText {
    autostart: &'static str,
    settings: &'static str,
    quit: &'static str,
}

thread_local! {
    static TRAY_MENU_HANDLES: RefCell<Option<TrayMenuItems>> = const { RefCell::new(None) };
}

impl TrayMenuItems {
    pub(crate) fn is_autostart(&self, id: &MenuId) -> bool {
        id == self.autostart.id()
    }

    pub(crate) fn is_settings(&self, id: &MenuId) -> bool {
        id == self.settings.id()
    }

    pub(crate) fn is_quit(&self, id: &MenuId) -> bool {
        id == self.quit.id()
    }

    pub(crate) fn set_autostart_checked(&self, enabled: bool) {
        self.autostart.set_checked(enabled);
    }
}

pub(crate) fn build(language: core::Language) -> Result<(TrayIcon, TrayMenuItems)> {
    let tray_menu = Menu::new();
    let text = tray_text(language);
    let autostart = CheckMenuItem::new(
        text.autostart,
        true,
        core::autostart_is_enabled().unwrap_or(false),
        None,
    );
    let settings = MenuItem::new(text.settings, true, None);
    let quit = MenuItem::new(text.quit, true, None);

    tray_menu.append_items(&[
        &autostart,
        &settings,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;

    let tray_items = TrayMenuItems {
        autostart,
        settings,
        quit,
    };

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(core::APP_NAME)
        .with_icon(make_icon()?)
        .build()
        .context("failed to create tray icon")?;

    Ok((tray, tray_items))
}

pub(crate) fn remember_handles(menu: &TrayMenuItems) {
    TRAY_MENU_HANDLES.with(|handles| {
        *handles.borrow_mut() = Some(menu.clone());
    });
}

pub(crate) fn clear_handles() {
    TRAY_MENU_HANDLES.with(|handles| {
        *handles.borrow_mut() = None;
    });
}

pub(crate) fn refresh_from_config(menu: &TrayMenuItems, config_path: &Path) {
    let config = core::load_config(config_path);
    apply_language(menu, config.language);
    refresh_autostart(menu);
}

pub(crate) fn apply_language(menu: &TrayMenuItems, language: core::Language) {
    let text = tray_text(language);
    menu.autostart.set_text(text.autostart);
    menu.settings.set_text(text.settings);
    menu.quit.set_text(text.quit);
}

pub(crate) fn refresh_autostart(menu: &TrayMenuItems) {
    menu.autostart
        .set_checked(core::autostart_is_enabled().unwrap_or(false));
}

pub(crate) fn preview_language(language: core::Language) -> bool {
    TRAY_MENU_HANDLES.with(|handles| {
        if let Some(menu) = handles.borrow().as_ref() {
            apply_language(menu, language);
            true
        } else {
            false
        }
    })
}

pub(crate) fn preview_autostart(enabled: bool) -> bool {
    TRAY_MENU_HANDLES.with(|handles| {
        if let Some(menu) = handles.borrow().as_ref() {
            menu.autostart.set_checked(enabled);
            true
        } else {
            false
        }
    })
}

fn tray_text(language: core::Language) -> TrayText {
    match language {
        core::Language::Vietnamese => TrayText {
            autostart: "Tự chạy cùng Windows",
            settings: "Cài đặt...",
            quit: "Thoát",
        },
        core::Language::English => TrayText {
            autostart: "Start with Windows",
            settings: "Settings...",
            quit: "Exit",
        },
    }
}

fn make_icon() -> Result<Icon> {
    let mut rgba = vec![0_u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let index = ((y * 32 + x) * 4) as usize;
            let dx = x - 16;
            let dy = y - 16;
            let inside = dx * dx + dy * dy <= 14 * 14;
            let color = if inside {
                [30, 144, 255, 255]
            } else {
                [0, 0, 0, 0]
            };
            rgba[index..index + 4].copy_from_slice(&color);
        }
    }
    Icon::from_rgba(rgba, 32, 32).context("failed to build tray icon")
}
