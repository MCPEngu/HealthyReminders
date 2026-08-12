use std::{
    env,
    fs::{self, File},
    io::{ErrorKind, Write},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use chrono::{Local, Timelike};
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming, WriteMode};
use serde::{Deserialize, Serialize};

const APP_NAME: &str = "HealthyReminders";
const CONFIG_FILE_NAME: &str = "HealthyReminders.json";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const DEFAULT_IDLE_TRIM_SECONDS: u64 = 60;
const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(5);
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;

unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
struct AppConfig {
    water_enabled: bool,
    eye_enabled: bool,
    movement_enabled: bool,
    water_interval_minutes: u64,
    eye_interval_minutes: u64,
    stand_interval_minutes: u64,
    stand_duration_minutes: u64,
    default_glass_ml: u64,
    water_goal_auto: bool,
    water_weight_kg: u64,
    water_gender: Gender,
    water_unit: WaterUnit,
    water_goal_ml_override: u64,
    movement_random_suggestion: bool,
    work_hours_enabled: bool,
    work_start_minutes: u16,
    work_end_minutes: u16,
    lunch_break_enabled: bool,
    lunch_start_minutes: u16,
    lunch_end_minutes: u16,
    language: Language,
    idle_trim_seconds: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            water_enabled: true,
            eye_enabled: true,
            movement_enabled: true,
            water_interval_minutes: 30,
            eye_interval_minutes: 20,
            stand_interval_minutes: 60,
            stand_duration_minutes: 5,
            default_glass_ml: 250,
            water_goal_auto: true,
            water_weight_kg: 70,
            water_gender: Gender::Other,
            water_unit: WaterUnit::Milliliter,
            water_goal_ml_override: 2300,
            movement_random_suggestion: true,
            work_hours_enabled: false,
            work_start_minutes: 9 * 60,
            work_end_minutes: 17 * 60 + 30,
            lunch_break_enabled: false,
            lunch_start_minutes: 12 * 60,
            lunch_end_minutes: 13 * 60,
            language: Language::Vietnamese,
            idle_trim_seconds: DEFAULT_IDLE_TRIM_SECONDS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
enum Language {
    #[default]
    Vietnamese,
    English,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
enum Gender {
    Female,
    Male,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
enum WaterUnit {
    #[default]
    Milliliter,
    Ounce,
}

#[derive(Debug)]
struct AppPaths {
    config_path: PathBuf,
    log_dir: PathBuf,
    lock_path: PathBuf,
}

#[derive(Debug)]
struct SingleInstanceGuard {
    _lock_file: File,
}

#[derive(Clone, Copy, Debug)]
enum ReminderKind {
    Water,
    Eyes,
    Stand,
    Work,
}

#[derive(Clone, Copy, Debug)]
struct DueTimes {
    water: Option<Instant>,
    eyes: Option<Instant>,
    stand: Option<Instant>,
    work: Option<Instant>,
}

impl DueTimes {
    fn from_schedule(config: &AppConfig, now: Instant) -> Self {
        Self::from_config(config, active_base(config, now))
    }

    fn from_config(config: &AppConfig, base: Instant) -> Self {
        Self {
            water: next_enabled_due(config.water_enabled, base, config.water_interval_minutes),
            eyes: next_enabled_due(config.eye_enabled, base, config.eye_interval_minutes),
            stand: next_enabled_due(config.movement_enabled, base, config.stand_interval_minutes),
            work: None,
        }
    }

    fn reset_from_schedule(&mut self, config: &AppConfig, now: Instant) {
        *self = Self::from_schedule(config, now);
    }

    fn next_due(&self, fallback: Instant) -> Instant {
        [self.water, self.eyes, self.stand, self.work]
            .into_iter()
            .flatten()
            .fold(fallback, Instant::min)
    }
}

pub fn main() {
    if let Err(error) = run_app() {
        eprintln!("{error:#}");
        log::error!("{error:#}");
        process::exit(1);
    }
}

fn run_app() -> Result<()> {
    let paths = resolve_paths()?;
    let _single_instance = acquire_single_instance(&paths.lock_path)?;
    init_logger(&paths)?;
    install_panic_hook();
    ensure_data_files(&paths)?;

    let config = load_config(&paths.config_path);
    log::info!(
        "starting {APP_NAME} Linux background reminders; config={}",
        paths.config_path.display()
    );
    println!(
        "{APP_NAME} Linux is running. Config: {}",
        paths.config_path.display()
    );

    run_loop(config, paths.config_path)
}

fn run_loop(mut config: AppConfig, config_path: PathBuf) -> Result<()> {
    let mut config_modified = modified_at(&config_path);
    let mut due = DueTimes::from_schedule(&config, Instant::now());

    loop {
        let latest_modified = modified_at(&config_path);
        if latest_modified != config_modified {
            config = load_config(&config_path);
            config_modified = latest_modified;
            due.reset_from_schedule(&config, Instant::now());
            log::info!("reloaded config from {}", config_path.display());
        }

        if !is_within_active_schedule(&config, current_minutes_of_day()) {
            let now = Instant::now();
            due.reset_from_schedule(&config, now);
            thread::sleep(next_timeout(&config, &due, Instant::now()));
            continue;
        }

        let now = Instant::now();
        if due.water.is_some_and(|due_at| now >= due_at) {
            show_reminder(ReminderKind::Water, &config);
            due.water = next_enabled_due(config.water_enabled, now, config.water_interval_minutes);
        }
        if due.eyes.is_some_and(|due_at| now >= due_at) {
            show_reminder(ReminderKind::Eyes, &config);
            due.eyes = next_enabled_due(config.eye_enabled, now, config.eye_interval_minutes);
        }
        if due.stand.is_some_and(|due_at| now >= due_at) {
            show_reminder(ReminderKind::Stand, &config);
            due.stand = None;
            due.work = config
                .movement_enabled
                .then_some(now + minutes(config.stand_duration_minutes));
        }
        if due.work.is_some_and(|due_at| now >= due_at) {
            show_reminder(ReminderKind::Work, &config);
            due.work = None;
            due.stand =
                next_enabled_due(config.movement_enabled, now, config.stand_interval_minutes);
        }

        thread::sleep(next_timeout(&config, &due, Instant::now()));
    }
}

fn show_reminder(kind: ReminderKind, config: &AppConfig) {
    let (title, body) = reminder_text(kind, config);
    if notify_send(&title, &body) {
        log::info!("shown Linux notification: {kind:?}");
    } else {
        println!("[{title}] {body}");
    }
}

fn notify_send(title: &str, body: &str) -> bool {
    let mut child = match Command::new("notify-send")
        .arg("-a")
        .arg(APP_NAME)
        .arg(title)
        .arg(body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            log::debug!("notify-send is unavailable: {error}");
            return false;
        }
    };

    wait_for_child_with_timeout(&mut child, NOTIFICATION_TIMEOUT)
}

fn wait_for_child_with_timeout(child: &mut process::Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return true,
            Ok(Some(status)) => {
                log::debug!("notify-send exited with {status}");
                return false;
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                log::warn!("notify-send timed out after {timeout:?}");
                if let Err(error) = child.kill() {
                    log::debug!("cannot terminate timed out notify-send: {error}");
                }
                let _ = child.wait();
                return false;
            }
            Err(error) => {
                log::debug!("cannot wait for notify-send: {error}");
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn reminder_text(kind: ReminderKind, config: &AppConfig) -> (String, String) {
    match (config.language, kind) {
        (Language::Vietnamese, ReminderKind::Water) => (
            "Uống nước".to_owned(),
            format!(
                "Uống một ly {}. Mục tiêu hôm nay khoảng {}.",
                format_water_volume(config.default_glass_ml, config.water_unit),
                format_water_volume(water_goal_ml(config), config.water_unit)
            ),
        ),
        (Language::Vietnamese, ReminderKind::Eyes) => (
            "Nghỉ mắt".to_owned(),
            "Nhìn xa khỏi màn hình trong 20 giây.".to_owned(),
        ),
        (Language::Vietnamese, ReminderKind::Stand) => (
            "Đứng dậy".to_owned(),
            movement_suggestion(config).to_owned(),
        ),
        (Language::Vietnamese, ReminderKind::Work) => (
            "Quay lại làm việc".to_owned(),
            "Hết giờ vận động, quay lại nhịp làm việc nhé.".to_owned(),
        ),
        (Language::English, ReminderKind::Water) => (
            "Drink water".to_owned(),
            format!(
                "Drink one {} glass. Today goal is about {}.",
                format_water_volume(config.default_glass_ml, config.water_unit),
                format_water_volume(water_goal_ml(config), config.water_unit)
            ),
        ),
        (Language::English, ReminderKind::Eyes) => (
            "Eye rest".to_owned(),
            "Look away from the screen for 20 seconds.".to_owned(),
        ),
        (Language::English, ReminderKind::Stand) => (
            "Stand up".to_owned(),
            movement_suggestion(config).to_owned(),
        ),
        (Language::English, ReminderKind::Work) => (
            "Back to work".to_owned(),
            "Movement break is done. Ease back into work.".to_owned(),
        ),
    }
}

fn movement_suggestion(config: &AppConfig) -> String {
    if !config.movement_random_suggestion {
        return match config.language {
            Language::Vietnamese => "Rời ghế, giãn cơ nhẹ hoặc đi lại một chút.".to_owned(),
            Language::English => {
                "Leave your chair, stretch lightly, or walk for a moment.".to_owned()
            }
        };
    }

    let suggestions = match config.language {
        Language::Vietnamese => [
            "Giãn lưng nhẹ trong 30 giây.",
            "Xoay cổ chậm 5 vòng mỗi bên.",
            "Đi bộ quanh phòng trong 1 phút.",
            "Duỗi vai và thả lỏng cánh tay.",
            "Đứng lên, hít sâu và vươn người.",
        ],
        Language::English => [
            "Stretch your back gently for 30 seconds.",
            "Do 5 slow neck rolls on each side.",
            "Walk around the room for 1 minute.",
            "Stretch your shoulders and relax your arms.",
            "Stand up, breathe deeply, and reach overhead.",
        ],
    };
    let index = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| (duration.as_secs() as usize) % suggestions.len())
        .unwrap_or(0);
    suggestions[index].to_owned()
}

fn resolve_paths() -> Result<AppPaths> {
    let config_base = xdg_dir("XDG_CONFIG_HOME", ".config")?.join(APP_NAME);
    let data_base = xdg_dir("XDG_DATA_HOME", ".local/share")?.join(APP_NAME);
    let lock_base = env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data_base.clone());

    Ok(AppPaths {
        config_path: config_base.join(CONFIG_FILE_NAME),
        log_dir: data_base.join("logs"),
        lock_path: lock_base.join(APP_NAME).join(format!("{APP_NAME}.lock")),
    })
}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    let mut builder = fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder
        .create(path)
        .with_context(|| format!("cannot create private directory {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("cannot secure directory {}", path.display()))
}

fn xdg_dir(variable: &str, fallback: &str) -> Result<PathBuf> {
    match env::var_os(variable) {
        Some(value) if !value.as_os_str().is_empty() => Ok(PathBuf::from(value)),
        _ => Ok(home_dir()?.join(fallback)),
    }
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| anyhow!("HOME is not set"))
}

fn ensure_data_files(paths: &AppPaths) -> Result<()> {
    if let Some(parent) = paths.config_path.parent() {
        create_private_dir(parent)?;
    }
    create_private_dir(&paths.log_dir)?;
    ensure_file_exists(&paths.config_path, "config", |path| {
        save_config(path, &AppConfig::default())
    })?;
    set_private_file_permissions(&paths.config_path)
}

fn ensure_file_exists(
    path: &Path,
    label: &str,
    write_default: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    if path
        .try_exists()
        .with_context(|| format!("cannot check {label} {}", path.display()))?
    {
        return Ok(());
    }

    write_default(path)
}

fn acquire_single_instance(lock_path: &Path) -> Result<SingleInstanceGuard> {
    if let Some(parent) = lock_path.parent() {
        create_private_dir(parent)?;
    }

    let lock_file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .with_context(|| format!("cannot open lock file {}", lock_path.display()))?;

    let lock_result = unsafe { flock(lock_file.as_raw_fd(), LOCK_EX | LOCK_NB) };
    if lock_result == 0 {
        return Ok(SingleInstanceGuard {
            _lock_file: lock_file,
        });
    }

    let error = std::io::Error::last_os_error();
    if error.kind() == ErrorKind::WouldBlock {
        Err(anyhow!("{APP_NAME} is already running"))
    } else {
        Err(error).with_context(|| format!("cannot lock {}", lock_path.display()))
    }
}

fn init_logger(paths: &AppPaths) -> Result<()> {
    create_private_dir(&paths.log_dir)?;

    Logger::try_with_env_or_str("info")?
        .log_to_file(
            FileSpec::default()
                .directory(&paths.log_dir)
                .basename(APP_NAME),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        .write_mode(WriteMode::BufferAndFlush)
        .rotate(
            Criterion::Size(5 * 1024 * 1024),
            Naming::Timestamps,
            Cleanup::KeepLogAndCompressedFiles(1, 4),
        )
        .cleanup_in_background_thread(true)
        .start()
        .map(|_| ())
        .context("failed to initialize flexi_logger")
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));
}

fn load_config(path: &Path) -> AppConfig {
    let config = match read_text_file_limited(path, MAX_CONFIG_BYTES) {
        Ok(Some(raw)) => serde_json::from_str(&raw).unwrap_or_else(|error| {
            log::warn!("invalid config at {}: {error}", path.display());
            AppConfig::default()
        }),
        Ok(None) => AppConfig::default(),
        Err(error) => {
            log::warn!("cannot read config at {}: {error:#}", path.display());
            AppConfig::default()
        }
    };
    normalize_config(config)
}

fn normalize_config(mut config: AppConfig) -> AppConfig {
    config.water_interval_minutes = clamp_stepped(config.water_interval_minutes, 5, 60, 5);
    config.eye_interval_minutes = clamp_stepped(config.eye_interval_minutes, 5, 60, 5);
    config.stand_interval_minutes = clamp_stepped(config.stand_interval_minutes, 15, 60, 5);
    config.stand_duration_minutes = config.stand_duration_minutes.clamp(1, 180);
    config.default_glass_ml = config.default_glass_ml.clamp(50, 2000);
    config.water_weight_kg = config.water_weight_kg.clamp(20, 250);
    config.water_goal_ml_override = config.water_goal_ml_override.clamp(500, 6000);
    config.work_start_minutes = config.work_start_minutes.min(1439);
    config.work_end_minutes = config.work_end_minutes.min(1439);
    config.lunch_start_minutes = config.lunch_start_minutes.min(1439);
    config.lunch_end_minutes = config.lunch_end_minutes.min(1439);
    config.idle_trim_seconds = config.idle_trim_seconds.max(10);
    config
}

fn save_config(path: &Path, config: &AppConfig) -> Result<()> {
    let config = normalize_config(config.clone());
    let raw = serde_json::to_string_pretty(&config).context("cannot serialize app config")?;
    write_text_atomic(path, &raw).with_context(|| format!("cannot write config {}", path.display()))
}

fn read_text_file_limited(path: &Path, max_bytes: u64) -> Result<Option<String>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("cannot stat {}", path.display())),
    };

    if !metadata.is_file() {
        return Err(anyhow!("{} is not a file", path.display()));
    }
    if metadata.len() > max_bytes {
        return Err(anyhow!("{} is too large", path.display()));
    }

    fs::read_to_string(path)
        .map(Some)
        .with_context(|| format!("cannot read {}", path.display()))
}

fn write_text_atomic(path: &Path, raw: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)?;
    }

    let temp_path = create_temp_file(path, raw)?;
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "cannot replace {} with {}",
            path.display(),
            temp_path.display()
        )
    })?;
    set_private_file_permissions(path)
}

fn create_temp_file(path: &Path, raw: &str) -> Result<PathBuf> {
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    for _ in 0..16 {
        let temp_path = parent.join(unique_temp_file_name("data-json"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp_path)
        {
            Ok(mut file) => {
                let write_result = file.write_all(raw.as_bytes()).and_then(|_| file.sync_all());
                drop(file);
                if let Err(error) = write_result {
                    let _ = fs::remove_file(&temp_path);
                    return Err(error).with_context(|| {
                        format!("cannot write temp file {}", temp_path.display())
                    });
                }
                return Ok(temp_path);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot create temp file {}", temp_path.display()));
            }
        }
    }

    Err(anyhow!(
        "cannot create unique temp file for {}",
        path.display()
    ))
}

fn set_private_file_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("cannot secure file {}", path.display()))
}

fn unique_temp_file_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(".{APP_NAME}.{prefix}.{}.{}.tmp", process::id(), nanos)
}

fn modified_at(path: &Path) -> Option<SystemTime> {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
}

fn minutes(value: u64) -> Duration {
    Duration::from_secs(value.max(1) * 60)
}

fn next_enabled_due(enabled: bool, base: Instant, interval_minutes: u64) -> Option<Instant> {
    enabled.then(|| base + minutes(interval_minutes))
}

fn next_timeout(config: &AppConfig, due_times: &DueTimes, now: Instant) -> Duration {
    let max_sleep = Duration::from_secs(config.idle_trim_seconds.clamp(10, 60));
    let minute = current_minutes_of_day();
    if !is_within_active_schedule(config, minute) {
        let wait_minutes = minutes_until_active_schedule(config, minute).max(1);
        return Duration::from_secs(wait_minutes * 60).min(max_sleep);
    }

    let next_due = due_times.next_due(now + max_sleep);
    next_due
        .saturating_duration_since(now)
        .max(Duration::from_secs(1))
}

fn active_base(config: &AppConfig, now: Instant) -> Instant {
    let minute = current_minutes_of_day();
    if is_within_active_schedule(config, minute) {
        return now;
    }

    let wait_minutes = minutes_until_active_schedule(config, minute).max(1);
    now + Duration::from_secs(wait_minutes * 60)
}

fn current_minutes_of_day() -> u16 {
    let now = Local::now();
    ((now.hour() * 60) + now.minute()) as u16
}

fn is_within_active_schedule(config: &AppConfig, minute: u16) -> bool {
    let minute = minute.min(1439);
    if config.work_hours_enabled {
        let start = config.work_start_minutes.min(1439);
        let end = config.work_end_minutes.min(1439);
        if start != end && !minute_in_range(minute, start, end) {
            return false;
        }
    }

    if config.lunch_break_enabled {
        let start = config.lunch_start_minutes.min(1439);
        let end = config.lunch_end_minutes.min(1439);
        if start != end && minute_in_range(minute, start, end) {
            return false;
        }
    }

    true
}

fn minutes_until_active_schedule(config: &AppConfig, minute: u16) -> u64 {
    let minute = minute.min(1439);
    if is_within_active_schedule(config, minute) {
        return 0;
    }

    for offset in 1..=1440u64 {
        let candidate = ((minute as u64 + offset) % 1440) as u16;
        if is_within_active_schedule(config, candidate) {
            return offset;
        }
    }

    60
}

fn minute_in_range(minute: u16, start: u16, end: u16) -> bool {
    if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    }
}

fn water_goal_ml(config: &AppConfig) -> u64 {
    if !config.water_goal_auto {
        return config.water_goal_ml_override.clamp(500, 6000);
    }

    let kg = config.water_weight_kg.clamp(20, 250);
    let per_kg = match config.water_gender {
        Gender::Female => 31,
        Gender::Male => 35,
        Gender::Other => 33,
    };
    round_to_step(kg * per_kg, 50).clamp(500, 6000)
}

fn format_water_volume(ml: u64, unit: WaterUnit) -> String {
    match unit {
        WaterUnit::Milliliter => format!("{ml} ml"),
        WaterUnit::Ounce => format!("{:.1} oz", ml as f64 / 29.5735),
    }
}

fn clamp_stepped(value: u64, min: u64, max: u64, step: u64) -> u64 {
    let value = value.clamp(min, max);
    let step = step.max(1);
    (((value + step / 2) / step) * step).clamp(min, max)
}

fn round_to_step(value: u64, step: u64) -> u64 {
    let step = step.max(1);
    ((value + (step / 2)) / step) * step
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(unique_temp_file_name(name))
    }

    #[test]
    fn normalize_config_clamps_linux_supported_fields() {
        let config = normalize_config(AppConfig {
            water_interval_minutes: 1,
            eye_interval_minutes: 62,
            stand_interval_minutes: 13,
            stand_duration_minutes: 0,
            default_glass_ml: 10,
            water_weight_kg: 500,
            water_goal_ml_override: 100,
            work_start_minutes: 2000,
            work_end_minutes: 2000,
            lunch_start_minutes: 2000,
            lunch_end_minutes: 2000,
            idle_trim_seconds: 1,
            ..AppConfig::default()
        });

        assert_eq!(config.water_interval_minutes, 5);
        assert_eq!(config.eye_interval_minutes, 60);
        assert_eq!(config.stand_interval_minutes, 15);
        assert_eq!(config.stand_duration_minutes, 1);
        assert_eq!(config.default_glass_ml, 50);
        assert_eq!(config.water_weight_kg, 250);
        assert_eq!(config.water_goal_ml_override, 500);
        assert_eq!(config.work_start_minutes, 1439);
        assert_eq!(config.work_end_minutes, 1439);
        assert_eq!(config.lunch_start_minutes, 1439);
        assert_eq!(config.lunch_end_minutes, 1439);
        assert_eq!(config.idle_trim_seconds, 10);
    }

    #[test]
    fn active_schedule_handles_work_hours_lunch_and_overnight_ranges() {
        let mut config = AppConfig {
            work_hours_enabled: true,
            work_start_minutes: 9 * 60,
            work_end_minutes: 17 * 60,
            lunch_break_enabled: true,
            lunch_start_minutes: 12 * 60,
            lunch_end_minutes: 13 * 60,
            ..AppConfig::default()
        };

        assert!(!is_within_active_schedule(&config, 8 * 60 + 59));
        assert!(is_within_active_schedule(&config, 9 * 60));
        assert!(!is_within_active_schedule(&config, 12 * 60 + 30));
        assert!(is_within_active_schedule(&config, 13 * 60));
        assert!(!is_within_active_schedule(&config, 17 * 60));

        config.work_start_minutes = 22 * 60;
        config.work_end_minutes = 6 * 60;
        config.lunch_break_enabled = false;

        assert!(is_within_active_schedule(&config, 23 * 60));
        assert!(is_within_active_schedule(&config, 5 * 60 + 59));
        assert!(!is_within_active_schedule(&config, 12 * 60));
    }

    #[test]
    fn minutes_until_active_schedule_returns_next_active_minute() {
        let mut config = AppConfig {
            work_hours_enabled: true,
            work_start_minutes: 9 * 60,
            work_end_minutes: 17 * 60,
            lunch_break_enabled: true,
            lunch_start_minutes: 12 * 60,
            lunch_end_minutes: 13 * 60,
            ..AppConfig::default()
        };

        assert_eq!(minutes_until_active_schedule(&config, 8 * 60 + 58), 2);
        assert_eq!(minutes_until_active_schedule(&config, 12 * 60 + 30), 30);

        config.work_start_minutes = 22 * 60;
        config.work_end_minutes = 6 * 60;
        config.lunch_break_enabled = false;

        assert_eq!(minutes_until_active_schedule(&config, 21 * 60 + 30), 30);
        assert_eq!(minutes_until_active_schedule(&config, 6 * 60), 16 * 60);
    }

    #[test]
    fn next_timeout_uses_earliest_due_time_or_idle_trim() {
        let now = Instant::now();
        let config = AppConfig {
            idle_trim_seconds: 60,
            ..AppConfig::default()
        };
        let due = DueTimes {
            water: Some(now + Duration::from_secs(20)),
            eyes: Some(now + Duration::from_secs(10)),
            stand: None,
            work: Some(now + Duration::from_secs(30)),
        };

        assert_eq!(next_timeout(&config, &due, now), Duration::from_secs(10));

        let no_due = DueTimes {
            water: None,
            eyes: None,
            stand: None,
            work: None,
        };

        assert_eq!(next_timeout(&config, &no_due, now), Duration::from_secs(60));
    }

    #[test]
    fn ensure_data_files_creates_default_config_only() {
        let dir = temp_test_dir("ensure-data-files-test");
        let paths = AppPaths {
            config_path: dir.join(CONFIG_FILE_NAME),
            log_dir: dir.join("logs"),
            lock_path: dir.join("lock"),
        };

        ensure_data_files(&paths).expect("default config should be created");

        assert!(paths.config_path.is_file());
        assert!(!dir.join("HealthyRemindersStats.json").exists());
        let config = load_config(&paths.config_path);
        assert_eq!(
            config.water_interval_minutes,
            AppConfig::default().water_interval_minutes
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn single_instance_lock_rejects_second_holder() {
        let dir = temp_test_dir("single-instance-test");
        fs::create_dir_all(&dir).expect("test directory should be created");
        let lock_path = dir.join("app.lock");

        let first = acquire_single_instance(&lock_path).expect("first lock should succeed");
        let second = acquire_single_instance(&lock_path);

        assert!(second.is_err());
        drop(first);

        acquire_single_instance(&lock_path).expect("lock should release after guard is dropped");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ensure_data_files_restricts_config_permissions() {
        let dir = temp_test_dir("private-config-test");
        let paths = AppPaths {
            config_path: dir.join("config").join(CONFIG_FILE_NAME),
            log_dir: dir.join("data").join("logs"),
            lock_path: dir.join("runtime").join("app.lock"),
        };

        ensure_data_files(&paths).expect("default config should be created securely");

        let config_mode = fs::metadata(&paths.config_path)
            .expect("config metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        let config_dir_mode = fs::metadata(paths.config_path.parent().unwrap())
            .expect("config directory metadata should be readable")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(config_mode, 0o600);
        assert_eq!(config_dir_mode, 0o700);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn child_timeout_terminates_a_stuck_notification_process() {
        let started = Instant::now();
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 5")
            .spawn()
            .expect("test child should start");

        assert!(!wait_for_child_with_timeout(
            &mut child,
            Duration::from_millis(25)
        ));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            child
                .try_wait()
                .expect("child status should be readable")
                .is_some()
        );
    }
}
