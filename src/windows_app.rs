use std::{
    sync::{
        Mutex, OnceLock,
        mpsc::{self, Sender},
    },
    thread,
};

use anyhow::{Context, Result};
use tao::{
    event::{Event, StartCause},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::windows::{WindowBuilderExtWindows, WindowExtWindows},
    window::WindowBuilder,
};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconEvent, menu::MenuEvent};
use windows::Win32::{
    Foundation::{HANDLE, HWND, LPARAM, LRESULT, WAIT_OBJECT_0, WPARAM},
    System::RemoteDesktop::{
        NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
    },
    System::Threading::{INFINITE, WaitForSingleObject},
    UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GWLP_WNDPROC, SetWindowLongPtrW, WM_ENDSESSION,
        WM_QUERYENDSESSION, WM_WTSSESSION_CHANGE, WNDPROC,
    },
};

use crate::{
    core, notifier,
    notifier::{NotifierCommand, NotifierHandle},
    scheduler,
    scheduler::{SchedulerCommand, SchedulerHandle},
    settings, tray,
};

#[derive(Clone, Debug)]
enum UserEvent {
    TrayIconEvent(TrayIconEvent),
    MenuEvent(MenuEvent),
    OpenSettings,
    RefreshTray,
    PreviewTrayLanguage(core::Language),
    PreviewTrayAutostart(bool),
    SessionLock,
    SessionUnlock,
    Shutdown,
}

struct Runtime {
    tray: TrayIcon,
    tray_menu: tray::TrayMenuItems,
    scheduler_tx: Sender<SchedulerCommand>,
    notifier_tx: Sender<NotifierCommand>,
    scheduler: Option<SchedulerHandle>,
    notifier: Option<NotifierHandle>,
    config_path: std::path::PathBuf,
    stats_path: std::path::PathBuf,
}

impl Runtime {
    fn shutdown(mut self, clean: bool) {
        if clean {
            let _ = self.notifier_tx.send(NotifierCommand::ClearAll);
        }
        let _ = self.scheduler_tx.send(SchedulerCommand::Shutdown);
        if let Some(scheduler) = self.scheduler.take() {
            scheduler.join();
        }
        if let Some(notifier) = self.notifier.take() {
            notifier.join();
        }
        tray::clear_handles();
        drop(self.tray);
        core::trim_memory();
    }
}

static EVENT_PROXY: OnceLock<Mutex<Option<EventLoopProxy<UserEvent>>>> = OnceLock::new();
static ORIGINAL_WNDPROC: OnceLock<isize> = OnceLock::new();
const WTS_SESSION_LOCK_EVENT: u32 = 0x7;
const WTS_SESSION_UNLOCK_EVENT: u32 = 0x8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionEndAction {
    Ignore,
    Allow,
    Shutdown,
}

pub fn main() {
    if let Err(error) = run_app() {
        log::error!("{error:#}");
    }
}

fn run_app() -> Result<()> {
    let _single_instance = core::setup_process()?;
    let paths = core::resolve_paths()?;
    core::ensure_data_files(&paths)?;
    let config = core::load_config(&paths.config_path);
    if let Err(error) = core::set_process_aumid(&config.aumid) {
        log::debug!("cannot set configured AppUserModelID: {error:#}");
    }
    if let Err(error) = core::spawn_toast_shortcut_setup(config.aumid.clone()) {
        log::warn!("cannot start toast shortcut setup: {error:#}");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    EVENT_PROXY.set(Mutex::new(Some(proxy.clone()))).ok();
    spawn_activation_watcher(_single_instance.activation_event_raw(), proxy.clone())?;

    TrayIconEvent::set_event_handler(Some({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_event(UserEvent::TrayIconEvent(event));
        }
    }));
    MenuEvent::set_event_handler(Some({
        let proxy = proxy.clone();
        move |event| {
            let _ = proxy.send_event(UserEvent::MenuEvent(event));
        }
    }));

    let hidden = WindowBuilder::new()
        .with_visible(false)
        .with_skip_taskbar(true)
        .with_drag_and_drop(false)
        .with_title(core::APP_NAME)
        .with_window_classname("HealthyRemindersMessageWindow")
        .build(&event_loop)
        .context("failed to create hidden tao window")?;
    let hwnd = HWND(hidden.hwnd());
    install_message_hook(hwnd);

    let (scheduler_tx, scheduler_rx) = mpsc::channel::<SchedulerCommand>();
    let (notifier_tx, notifier_rx) = mpsc::channel::<NotifierCommand>();

    let notifier =
        notifier::spawn_notifier(config.aumid.clone(), notifier_rx, scheduler_tx.clone())?;
    let scheduler = scheduler::spawn_scheduler(
        config.clone(),
        scheduler_rx,
        notifier_tx.clone(),
        paths.stats_path.clone(),
    )?;
    let _watcher = core::spawn_config_watcher(paths.config_path.clone(), scheduler_tx.clone())?;

    let (tray, tray_menu) = tray::build(config.language)?;
    tray::remember_handles(&tray_menu);
    let mut runtime = Some(Runtime {
        tray,
        tray_menu,
        scheduler_tx,
        notifier_tx,
        scheduler: Some(scheduler),
        notifier: Some(notifier),
        config_path: paths.config_path,
        stats_path: paths.stats_path,
    });

    if !core::launch_minimized_arg()
        && let Some(runtime_ref) = runtime.as_ref()
    {
        open_settings(runtime_ref);
    }

    core::debounce_trim();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {}
            Event::UserEvent(UserEvent::TrayIconEvent(event)) if should_open_settings(&event) => {
                if let Some(runtime_ref) = runtime.as_ref() {
                    open_settings(runtime_ref);
                }
            }
            Event::UserEvent(UserEvent::OpenSettings) => {
                if let Some(runtime_ref) = runtime.as_ref() {
                    open_settings(runtime_ref);
                }
            }
            Event::UserEvent(UserEvent::RefreshTray) => {
                if let Some(runtime_ref) = runtime.as_ref() {
                    tray::refresh_from_config(&runtime_ref.tray_menu, &runtime_ref.config_path);
                    settings::refresh_autostart_checkbox();
                }
            }
            Event::UserEvent(UserEvent::PreviewTrayLanguage(language)) => {
                if let Some(runtime_ref) = runtime.as_ref() {
                    tray::apply_language(&runtime_ref.tray_menu, language);
                }
            }
            Event::UserEvent(UserEvent::PreviewTrayAutostart(enabled)) => {
                if let Some(runtime_ref) = runtime.as_ref() {
                    runtime_ref.tray_menu.set_autostart_checked(enabled);
                }
            }
            Event::UserEvent(UserEvent::SessionLock) => {
                if let Some(runtime) = runtime.as_ref() {
                    let _ = runtime.scheduler_tx.send(SchedulerCommand::Lock);
                }
            }
            Event::UserEvent(UserEvent::SessionUnlock) => {
                if let Some(runtime) = runtime.as_ref() {
                    let _ = runtime.scheduler_tx.send(SchedulerCommand::Unlock);
                }
            }
            Event::UserEvent(UserEvent::Shutdown) => {
                if let Some(runtime) = runtime.take() {
                    runtime.shutdown(true);
                }
                unsafe {
                    let _ = WTSUnRegisterSessionNotification(hwnd);
                }
                *control_flow = ControlFlow::Exit;
            }
            Event::UserEvent(UserEvent::MenuEvent(event)) => {
                if let Some(runtime_ref) = runtime.as_mut() {
                    handle_menu_event(event, runtime_ref, control_flow);
                }
            }
            Event::LoopDestroyed => {
                if let Some(runtime) = runtime.take() {
                    runtime.shutdown(true);
                }
                unsafe {
                    let _ = WTSUnRegisterSessionNotification(hwnd);
                }
                if let Some(lock) = EVENT_PROXY.get()
                    && let Ok(mut slot) = lock.lock()
                {
                    *slot = None;
                }
            }
            _ => {}
        }
    });
}

fn handle_menu_event(event: MenuEvent, runtime: &mut Runtime, control_flow: &mut ControlFlow) {
    let id = event.id();
    if runtime.tray_menu.is_autostart(id) {
        let target = !core::autostart_is_enabled().unwrap_or(false);
        if let Err(error) = core::set_autostart(target) {
            log::warn!("autostart toggle failed: {error:#}");
            tray::refresh_autostart(&runtime.tray_menu);
        } else {
            runtime.tray_menu.set_autostart_checked(target);
        }
        settings::refresh_autostart_checkbox();
    } else if runtime.tray_menu.is_settings(id) {
        open_settings(runtime);
    } else if runtime.tray_menu.is_quit(id) {
        let _ = runtime.notifier_tx.send(NotifierCommand::ClearAll);
        let _ = runtime.scheduler_tx.send(SchedulerCommand::Shutdown);
        *control_flow = ControlFlow::Exit;
    }
}

fn open_settings(runtime: &Runtime) {
    tray::refresh_autostart(&runtime.tray_menu);
    if let Err(error) = settings::show_settings(settings::SettingsContext {
        config_path: runtime.config_path.clone(),
        stats_path: runtime.stats_path.clone(),
        scheduler_tx: runtime.scheduler_tx.clone(),
        notifier_tx: runtime.notifier_tx.clone(),
    }) {
        log::warn!("settings window failed: {error:#}");
    }
}

pub(crate) fn request_tray_refresh() {
    send_user_event(UserEvent::RefreshTray);
}

pub(crate) fn preview_tray_language(language: core::Language) {
    if !tray::preview_language(language) {
        send_user_event(UserEvent::PreviewTrayLanguage(language));
    }
}

pub(crate) fn preview_tray_autostart(enabled: bool) {
    if !tray::preview_autostart(enabled) {
        send_user_event(UserEvent::PreviewTrayAutostart(enabled));
    }
}

fn should_open_settings(event: &TrayIconEvent) -> bool {
    match event {
        TrayIconEvent::Click {
            button,
            button_state,
            ..
        } => *button == MouseButton::Left && *button_state == MouseButtonState::Up,
        TrayIconEvent::DoubleClick { button, .. } => *button == MouseButton::Left,
        _ => false,
    }
}

fn spawn_activation_watcher(event_handle: isize, proxy: EventLoopProxy<UserEvent>) -> Result<()> {
    thread::Builder::new()
        .name("ActivationWatcher".to_owned())
        .spawn(move || {
            loop {
                let wait_result = unsafe { WaitForSingleObject(HANDLE(event_handle), INFINITE) };
                if wait_result != WAIT_OBJECT_0 {
                    break;
                }
                if proxy.send_event(UserEvent::OpenSettings).is_err() {
                    break;
                }
            }
        })
        .context("failed to spawn ActivationWatcher")?;
    Ok(())
}

fn install_message_hook(hwnd: HWND) {
    unsafe {
        let original = SetWindowLongPtrW(
            hwnd,
            GWLP_WNDPROC,
            healthy_reminders_wnd_proc as *const () as isize,
        );
        ORIGINAL_WNDPROC.set(original).ok();
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
    }
}

unsafe extern "system" fn healthy_reminders_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match std::panic::catch_unwind(|| healthy_reminders_wnd_proc_inner(hwnd, msg, wparam, lparam)) {
        Ok(result) => result,
        Err(_) => {
            log::error!("message window procedure panicked");
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

fn healthy_reminders_wnd_proc_inner(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match session_end_action(msg, wparam.0 != 0) {
        SessionEndAction::Allow => return LRESULT(1),
        SessionEndAction::Shutdown => {
            send_user_event(UserEvent::Shutdown);
            return LRESULT(0);
        }
        SessionEndAction::Ignore => {}
    }

    if msg == WM_WTSSESSION_CHANGE {
        match wparam.0 as u32 {
            WTS_SESSION_LOCK_EVENT => send_user_event(UserEvent::SessionLock),
            WTS_SESSION_UNLOCK_EVENT => send_user_event(UserEvent::SessionUnlock),
            _ => {}
        }
    }

    if let Some(original) = ORIGINAL_WNDPROC.get().copied()
        && original != 0
    {
        let proc: WNDPROC = unsafe { std::mem::transmute(original) };
        return unsafe { CallWindowProcW(proc, hwnd, msg, wparam, lparam) };
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn session_end_action(msg: u32, session_is_ending: bool) -> SessionEndAction {
    match msg {
        WM_QUERYENDSESSION => SessionEndAction::Allow,
        WM_ENDSESSION if session_is_ending => SessionEndAction::Shutdown,
        _ => SessionEndAction::Ignore,
    }
}

fn send_user_event(event: UserEvent) {
    if let Some(lock) = EVENT_PROXY.get()
        && let Ok(slot) = lock.lock()
        && let Some(proxy) = slot.as_ref()
    {
        let _ = proxy.send_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_query_allows_shutdown_without_requesting_app_exit() {
        assert_eq!(
            session_end_action(WM_QUERYENDSESSION, false),
            SessionEndAction::Allow
        );
        assert_eq!(
            session_end_action(WM_ENDSESSION, false),
            SessionEndAction::Ignore
        );
        assert_eq!(
            session_end_action(WM_ENDSESSION, true),
            SessionEndAction::Shutdown
        );
    }
}
