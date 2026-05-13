use std::{
    sync::mpsc::{Receiver, Sender},
    thread,
};

use anyhow::{Context, Result};
use windows::{
    Data::Xml::Dom::XmlDocument,
    Foundation::{EventRegistrationToken, TypedEventHandler},
    UI::Notifications::{ToastNotification, ToastNotificationManager},
    core::{HSTRING, IInspectable},
};

use crate::{
    core::{self, AppConfig},
    overlay,
    scheduler::{ReminderKind, SchedulerCommand},
};

#[derive(Clone, Debug)]
pub enum NotifierCommand {
    Show {
        kind: ReminderKind,
        config: AppConfig,
    },
    ShowAndReport {
        kind: ReminderKind,
        config: AppConfig,
        reply: Sender<std::result::Result<(), String>>,
    },
    ClearAll,
    Shutdown,
}

pub struct NotifierHandle {
    join: Option<thread::JoinHandle<()>>,
}

impl NotifierHandle {
    pub fn join(mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct ActiveToast {
    _toast: ToastNotification,
    _token: EventRegistrationToken,
}

pub fn spawn_notifier(
    aumid: String,
    rx: Receiver<NotifierCommand>,
    scheduler_tx: Sender<SchedulerCommand>,
) -> Result<NotifierHandle> {
    let join = thread::Builder::new()
        .name("NotifierThread".to_owned())
        .spawn(move || run(aumid, rx, scheduler_tx))
        .context("failed to spawn NotifierThread")?;

    Ok(NotifierHandle { join: Some(join) })
}

pub fn clear_all_notifications(aumid: &str) -> Result<()> {
    core::init_com_runtime()?;
    let history =
        ToastNotificationManager::History().context("ToastNotificationManager::History failed")?;
    history
        .ClearWithId(&HSTRING::from(aumid))
        .context("ToastNotificationHistory::ClearWithId failed")
}

fn run(aumid: String, rx: Receiver<NotifierCommand>, scheduler_tx: Sender<SchedulerCommand>) {
    if let Err(error) = core::init_com_runtime() {
        log::warn!("COM init for notifier failed: {error:#}");
        return;
    }

    let mut active_toasts: Vec<ActiveToast> = Vec::with_capacity(4);

    while let Ok(command) = rx.recv() {
        match command {
            NotifierCommand::Show { kind, config } => {
                match show_reminder(&aumid, kind, &config, scheduler_tx.clone()) {
                    Ok(Some(active)) => retain_active_toast(&mut active_toasts, active),
                    Ok(None) => {}
                    Err(error) => log::warn!("toast failed: {error:#}"),
                }
            }
            NotifierCommand::ShowAndReport {
                kind,
                config,
                reply,
            } => {
                let result = match show_reminder(&aumid, kind, &config, scheduler_tx.clone()) {
                    Ok(Some(active)) => {
                        retain_active_toast(&mut active_toasts, active);
                        Ok(())
                    }
                    Ok(None) => Ok(()),
                    Err(error) => {
                        let message = format!("{error:#}");
                        log::warn!("toast failed: {message}");
                        Err(message)
                    }
                };
                let _ = reply.send(result);
            }
            NotifierCommand::ClearAll => {
                if let Err(error) = clear_all_notifications(&aumid) {
                    log::debug!("clear notifications failed: {error:#}");
                }
                active_toasts.clear();
            }
            NotifierCommand::Shutdown => {
                let _ = clear_all_notifications(&aumid);
                break;
            }
        }
    }
}

fn retain_active_toast(active_toasts: &mut Vec<ActiveToast>, active: ActiveToast) {
    active_toasts.push(active);
    if active_toasts.len() > 8 {
        active_toasts.remove(0);
    }
}

fn show_reminder(
    aumid: &str,
    kind: ReminderKind,
    config: &AppConfig,
    scheduler_tx: Sender<SchedulerCommand>,
) -> Result<Option<ActiveToast>> {
    if config.full_screen_reminders {
        match overlay::spawn_overlay(kind, config.clone(), scheduler_tx.clone()) {
            Ok(()) => return Ok(None),
            Err(error) => log::warn!("overlay failed, falling back to toast: {error:#}"),
        }
    }

    show_toast(aumid, kind, config, scheduler_tx).map(Some)
}

fn show_toast(
    aumid: &str,
    kind: ReminderKind,
    config: &AppConfig,
    scheduler_tx: Sender<SchedulerCommand>,
) -> Result<ActiveToast> {
    let xml = XmlDocument::new().context("XmlDocument::new failed")?;
    xml.LoadXml(&HSTRING::from(toast_xml(kind, config)))
        .context("XmlDocument::LoadXml failed")?;

    let toast = ToastNotification::CreateToastNotification(&xml)
        .context("CreateToastNotification failed")?;
    toast.SetTag(&HSTRING::from(tag(kind))).ok();
    toast.SetGroup(&HSTRING::from(core::APP_NAME)).ok();

    let token = toast
        .Activated(&TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_sender, _args| {
                let _ = scheduler_tx.send(SchedulerCommand::ToastActivated(kind));
                Ok(())
            },
        ))
        .context("ToastNotification::Activated failed")?;

    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid))
        .context("CreateToastNotifierWithId failed")?;
    notifier
        .Show(&toast)
        .context("ToastNotifier::Show failed")?;

    Ok(ActiveToast {
        _toast: toast,
        _token: token,
    })
}

fn toast_xml(kind: ReminderKind, config: &AppConfig) -> String {
    let sound = config.sound && !(kind == ReminderKind::Eyes && config.eye_silent_mode);
    let audio = if sound {
        r#"<audio src="ms-winsoundevent:Notification.Reminder"/>"#
    } else {
        r#"<audio silent="true"/>"#
    };

    let (title, body) = toast_text(kind, config);
    match kind {
        ReminderKind::Water => format!(
            r#"<toast duration="short" launch="water">
  <visual>
    <binding template="ToastGeneric">
      <text>{title}</text>
      <text>{body}</text>
    </binding>
  </visual>
  {audio}
</toast>"#
        ),
        ReminderKind::Eyes => format!(
            r#"<toast duration="short" launch="eyes">
  <visual>
    <binding template="ToastGeneric">
      <text>{title}</text>
      <text>{body}</text>
    </binding>
  </visual>
  {audio}
</toast>"#
        ),
        ReminderKind::Stand => format!(
            r#"<toast duration="short" launch="stand">
  <visual>
    <binding template="ToastGeneric">
      <text>{title}</text>
      <text>{body}</text>
    </binding>
  </visual>
  {audio}
</toast>"#
        ),
        ReminderKind::Work => format!(
            r#"<toast duration="short" launch="work">
  <visual>
    <binding template="ToastGeneric">
      <text>{title}</text>
      <text>{body}</text>
    </binding>
  </visual>
  {audio}
</toast>"#
        ),
    }
}

fn toast_text(kind: ReminderKind, config: &AppConfig) -> (&'static str, String) {
    match (config.language, kind) {
        (core::Language::Vietnamese, ReminderKind::Water) => {
            ("Uống nước", "Đã đến lúc bổ sung nước.".to_owned())
        }
        (core::Language::Vietnamese, ReminderKind::Eyes) => (
            "Nghỉ mắt",
            format!(
                "Nhìn xa {} giây để giảm mỏi mắt.",
                config.eye_countdown_seconds.clamp(10, 60)
            ),
        ),
        (core::Language::Vietnamese, ReminderKind::Stand) => {
            ("Đứng dậy", core::movement_suggestion(config).to_owned())
        }
        (core::Language::Vietnamese, ReminderKind::Work) => (
            "Quay lại làm việc",
            "Hết thời gian đứng dậy. Tiếp tục công việc thôi.".to_owned(),
        ),
        (core::Language::English, ReminderKind::Water) => {
            ("Drink water", "Time to hydrate.".to_owned())
        }
        (core::Language::English, ReminderKind::Eyes) => (
            "Eye rest",
            format!(
                "Look away for {} seconds to reduce eye strain.",
                config.eye_countdown_seconds.clamp(10, 60)
            ),
        ),
        (core::Language::English, ReminderKind::Stand) => {
            ("Stand up", core::movement_suggestion(config).to_owned())
        }
        (core::Language::English, ReminderKind::Work) => (
            "Back to work",
            "Your movement break is over. Time to get back to work.".to_owned(),
        ),
    }
}

fn tag(kind: ReminderKind) -> &'static str {
    match kind {
        ReminderKind::Water => "water",
        ReminderKind::Eyes => "eyes",
        ReminderKind::Stand => "stand",
        ReminderKind::Work => "work",
    }
}
