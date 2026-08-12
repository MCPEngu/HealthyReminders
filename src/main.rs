#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod core;
#[cfg(target_os = "linux")]
mod linux_app;
#[cfg(windows)]
mod notifier;
#[cfg(windows)]
mod overlay;
#[cfg(windows)]
mod scheduler;
#[cfg(windows)]
mod settings;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod windows_app;

#[cfg(windows)]
pub(crate) use windows_app::{preview_tray_autostart, preview_tray_language, request_tray_refresh};

#[cfg(windows)]
fn main() {
    windows_app::main();
}

#[cfg(target_os = "linux")]
fn main() {
    linux_app::main();
}

#[cfg(not(any(windows, target_os = "linux")))]
fn main() {
    eprintln!("HealthyReminders supports Windows and Linux builds.");
}
