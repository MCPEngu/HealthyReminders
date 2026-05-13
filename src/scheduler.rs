use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{core, notifier::NotifierCommand};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReminderKind {
    Water,
    Eyes,
    Stand,
    Work,
}

#[derive(Clone, Debug)]
pub enum SchedulerCommand {
    Reload(core::AppConfig),
    Lock,
    Unlock,
    ToastActivated(ReminderKind),
    Snooze(ReminderKind, u64),
    Shutdown,
}

pub struct SchedulerHandle {
    join: Option<thread::JoinHandle<()>>,
}

impl SchedulerHandle {
    pub fn join(mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub fn spawn_scheduler(
    initial_config: core::AppConfig,
    rx: Receiver<SchedulerCommand>,
    notifier_tx: Sender<NotifierCommand>,
    stats_path: PathBuf,
) -> Result<SchedulerHandle> {
    let join = thread::Builder::new()
        .name("SchedulerThread".to_owned())
        .spawn(move || run(initial_config, rx, notifier_tx, stats_path))
        .context("failed to spawn SchedulerThread")?;

    Ok(SchedulerHandle { join: Some(join) })
}

#[derive(Clone, Copy, Debug)]
struct DueTimes {
    water: Option<Instant>,
    eyes: Option<Instant>,
    stand: Option<Instant>,
    work: Option<Instant>,
}

impl DueTimes {
    fn from_config(config: &core::AppConfig, base: Instant) -> Self {
        Self {
            water: next_enabled_due(config.water_enabled, base, config.water_interval_minutes),
            eyes: next_enabled_due(config.eye_enabled, base, config.eye_interval_minutes),
            stand: next_enabled_due(config.movement_enabled, base, config.stand_interval_minutes),
            work: None,
        }
    }

    fn reset(&mut self, config: &core::AppConfig, base: Instant) {
        *self = Self::from_config(config, base);
    }

    fn shift_by(&mut self, duration: Duration) {
        if let Some(due) = self.water.as_mut() {
            *due += duration;
        }
        if let Some(due) = self.eyes.as_mut() {
            *due += duration;
        }
        if let Some(due) = self.stand.as_mut() {
            *due += duration;
        }
        if let Some(due) = self.work.as_mut() {
            *due += duration;
        }
    }
}

fn run(
    mut config: core::AppConfig,
    rx: Receiver<SchedulerCommand>,
    notifier_tx: Sender<NotifierCommand>,
    stats_path: PathBuf,
) {
    let base = active_base(&config, Instant::now());
    let mut due = DueTimes::from_config(&config, base);
    let mut locked_at: Option<Instant> = None;
    let mut next_trim = Instant::now() + Duration::from_secs(config.idle_trim_seconds.max(10));

    loop {
        let now = Instant::now();
        let timeout = next_timeout(&config, locked_at, &due, next_trim, now);

        match rx.recv_timeout(timeout) {
            Ok(SchedulerCommand::Reload(new_config)) => {
                config = new_config;
                let now = Instant::now();
                let base = active_base(&config, now);
                due.reset(&config, base);
                next_trim = now + Duration::from_secs(config.idle_trim_seconds.max(10));
                core::trim_memory();
            }
            Ok(SchedulerCommand::Lock) => {
                if locked_at.is_none() {
                    locked_at = Some(Instant::now());
                }
                core::trim_memory();
            }
            Ok(SchedulerCommand::Unlock) => {
                if let Some(start) = locked_at.take() {
                    let frozen = Instant::now().saturating_duration_since(start);
                    due.shift_by(frozen);
                }
            }
            Ok(SchedulerCommand::ToastActivated(kind)) => {
                log::info!("toast activated: {kind:?}");
                record_activation(&stats_path, &config, kind);
                let base = active_base(&config, Instant::now());
                match kind {
                    ReminderKind::Water => {
                        due.water = next_enabled_due(
                            config.water_enabled,
                            base,
                            config.water_interval_minutes,
                        );
                    }
                    ReminderKind::Eyes => {
                        due.eyes =
                            next_enabled_due(config.eye_enabled, base, config.eye_interval_minutes);
                    }
                    ReminderKind::Stand => {}
                    ReminderKind::Work => {
                        due.work = None;
                        due.stand = next_enabled_due(
                            config.movement_enabled,
                            base,
                            config.stand_interval_minutes,
                        );
                    }
                }
                core::trim_memory();
            }
            Ok(SchedulerCommand::Snooze(kind, minutes_value)) => {
                log::info!("snooze requested: {kind:?} for {minutes_value} minutes");
                let due_at = Instant::now() + minutes(minutes_value);
                match kind {
                    ReminderKind::Water => {
                        due.water = config.water_enabled.then_some(due_at);
                    }
                    ReminderKind::Eyes => {
                        due.eyes = config.eye_enabled.then_some(due_at);
                    }
                    ReminderKind::Stand | ReminderKind::Work => {
                        due.work = None;
                        due.stand = config.movement_enabled.then_some(due_at);
                    }
                }
                core::trim_memory();
            }
            Ok(SchedulerCommand::Shutdown) => break,
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if locked_at.is_some() {
                    core::trim_memory();
                    continue;
                }

                let now = Instant::now();
                if !core::is_within_active_schedule(&config, core::current_minutes_of_day()) {
                    let base = active_base(&config, now);
                    due.reset(&config, base);
                    if now >= next_trim {
                        core::trim_memory();
                        next_trim = now + Duration::from_secs(config.idle_trim_seconds.max(10));
                    }
                    continue;
                }

                if due.water.is_some_and(|due_at| now >= due_at) {
                    let _ = notifier_tx.send(NotifierCommand::Show {
                        kind: ReminderKind::Water,
                        config: config.clone(),
                    });
                    due.water =
                        next_enabled_due(config.water_enabled, now, config.water_interval_minutes);
                }
                if due.eyes.is_some_and(|due_at| now >= due_at) {
                    let _ = notifier_tx.send(NotifierCommand::Show {
                        kind: ReminderKind::Eyes,
                        config: config.clone(),
                    });
                    due.eyes =
                        next_enabled_due(config.eye_enabled, now, config.eye_interval_minutes);
                }
                if due.stand.is_some_and(|due_at| now >= due_at) {
                    let _ = notifier_tx.send(NotifierCommand::Show {
                        kind: ReminderKind::Stand,
                        config: config.clone(),
                    });
                    due.stand = None;
                    due.work = if config.movement_enabled {
                        Some(now + minutes(config.stand_duration_minutes))
                    } else {
                        None
                    };
                }
                if due.work.is_some_and(|due_at| now >= due_at) {
                    let _ = notifier_tx.send(NotifierCommand::Show {
                        kind: ReminderKind::Work,
                        config: config.clone(),
                    });
                    due.work = None;
                    due.stand = next_enabled_due(
                        config.movement_enabled,
                        now,
                        config.stand_interval_minutes,
                    );
                }
                if now >= next_trim {
                    core::trim_memory();
                    next_trim = now + Duration::from_secs(config.idle_trim_seconds.max(10));
                }
            }
        }
    }

    let _ = notifier_tx.send(NotifierCommand::ClearAll);
    let _ = notifier_tx.send(NotifierCommand::Shutdown);
}

fn minutes(value: u64) -> Duration {
    Duration::from_secs(value.max(1) * 60)
}

fn next_enabled_due(enabled: bool, base: Instant, interval_minutes: u64) -> Option<Instant> {
    enabled.then(|| base + minutes(interval_minutes))
}

fn next_timeout(
    config: &core::AppConfig,
    locked_at: Option<Instant>,
    due_times: &DueTimes,
    next_trim: Instant,
    now: Instant,
) -> Duration {
    if locked_at.is_some() {
        return Duration::from_secs(config.idle_trim_seconds.max(10));
    }

    if !core::is_within_active_schedule(config, core::current_minutes_of_day()) {
        let wait_minutes =
            core::minutes_until_active_schedule(config, core::current_minutes_of_day()).max(1);
        let schedule_due = now + Duration::from_secs(wait_minutes * 60);
        return schedule_due.min(next_trim).saturating_duration_since(now);
    }

    let mut next_due = next_trim;
    if let Some(due) = due_times.water {
        next_due = next_due.min(due);
    }
    if let Some(due) = due_times.eyes {
        next_due = next_due.min(due);
    }
    if let Some(due) = due_times.stand {
        next_due = next_due.min(due);
    }
    if let Some(due) = due_times.work {
        next_due = next_due.min(due);
    }
    next_due.saturating_duration_since(now)
}

fn active_base(config: &core::AppConfig, now: Instant) -> Instant {
    if core::is_within_active_schedule(config, core::current_minutes_of_day()) {
        return now;
    }

    let wait_minutes =
        core::minutes_until_active_schedule(config, core::current_minutes_of_day()).max(1);
    now + Duration::from_secs(wait_minutes * 60)
}

fn record_activation(stats_path: &std::path::Path, config: &core::AppConfig, kind: ReminderKind) {
    let activity = match kind {
        ReminderKind::Water => Some(core::ActivityKind::Water {
            ml: config.default_glass_ml.max(1),
        }),
        ReminderKind::Eyes => Some(core::ActivityKind::EyeRest),
        ReminderKind::Stand => None,
        ReminderKind::Work => Some(core::ActivityKind::Movement),
    };

    if let Some(activity) = activity {
        if let Err(error) = core::record_activity(stats_path, activity) {
            log::warn!("cannot record activity: {error:#}");
        }
    }
}
