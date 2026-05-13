use std::{
    env,
    ffi::OsStr,
    fs,
    io::{ErrorKind, Write},
    os::windows::ffi::OsStrExt,
    panic,
    path::{Path, PathBuf},
    process,
    sync::mpsc::Sender,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, Context, Result};
use chrono::{Duration as ChronoDuration, Local, NaiveDate, Timelike};
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming, WriteMode};
use notify::{recommended_watcher, Event, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use windows::{
    core::{ComInterface, PCWSTR, PWSTR},
    Win32::{
        Foundation::{
            CloseHandle, GetLastError, BOOL, ERROR_ALREADY_EXISTS, HANDLE, RPC_E_CHANGED_MODE,
        },
        Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH},
        System::{
            Com::StructuredStorage::{
                PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
            },
            Com::{
                CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile,
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED,
            },
            ProcessStatus::EmptyWorkingSet,
            Registry::{
                RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
                HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_DWORD, REG_SZ,
                REG_VALUE_TYPE,
            },
            Threading::{
                CreateEventW, CreateMutexW, GetCurrentProcess, OpenEventW, ProcessPowerThrottling,
                SetEvent, SetProcessInformation, EVENT_MODIFY_STATE,
                PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                PROCESS_POWER_THROTTLING_STATE,
            },
            Variant::VT_LPWSTR,
        },
        UI::Shell::{
            IShellLinkW,
            PropertiesSystem::{IPropertyStore, PROPERTYKEY},
            SetCurrentProcessExplicitAppUserModelID, ShellLink,
        },
    },
};

pub const APP_NAME: &str = "HealthyReminders";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
pub const AUMID: &str = "MCPEngu1.HealthyReminders.Portable";
pub const AUTOSTART_VALUE_NAME: &str = "HealthyReminders";
pub const DEFAULT_IDLE_TRIM_SECONDS: u64 = 60;
pub const GITHUB_RELEASES_URL: &str = "https://github.com/MCPEngu/HealthyReminders/releases";
const CONFIG_FILE_NAME: &str = "HealthyReminders.json";
const STATS_FILE_NAME: &str = "HealthyRemindersStats.json";
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_STATS_BYTES: u64 = 1024 * 1024;
const MAX_DAILY_WATER_ML: u64 = 100_000;
const MAX_DAILY_COUNT: u64 = 1_000;
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const PERSONALIZE_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
const APPS_USE_LIGHT_THEME: &str = "AppsUseLightTheme";
const MUTEX_NAME: &str = "HealthyReminders_Global_Mutex";
const ACTIVATION_EVENT_NAME: &str = "HealthyReminders_Open_Settings_Event";
const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct AppConfig {
    pub water_enabled: bool,
    pub eye_enabled: bool,
    pub movement_enabled: bool,
    pub water_interval_minutes: u64,
    pub eye_interval_minutes: u64,
    pub stand_interval_minutes: u64,
    pub stand_duration_minutes: u64,
    pub default_glass_ml: u64,
    pub water_goal_auto: bool,
    pub water_weight_kg: u64,
    pub water_gender: Gender,
    pub water_unit: WaterUnit,
    pub water_goal_ml_override: u64,
    pub eye_countdown_seconds: u64,
    pub eye_silent_mode: bool,
    pub movement_random_suggestion: bool,
    pub movement_snooze_minutes: u64,
    pub work_hours_enabled: bool,
    pub work_start_minutes: u16,
    pub work_end_minutes: u16,
    pub lunch_break_enabled: bool,
    pub lunch_start_minutes: u16,
    pub lunch_end_minutes: u16,
    pub theme: ThemeMode,
    pub language: Language,
    pub minimal_mode: bool,
    pub full_screen_reminders: bool,
    pub focus_mode_enabled: bool,
    pub focus_countdown_seconds: u64,
    pub overlay_style: OverlayStyle,
    pub overlay_accent: OverlayAccent,
    pub idle_trim_seconds: u64,
    pub aumid: String,
    pub sound: bool,
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
            eye_countdown_seconds: 20,
            eye_silent_mode: false,
            movement_random_suggestion: true,
            movement_snooze_minutes: 10,
            work_hours_enabled: false,
            work_start_minutes: 9 * 60,
            work_end_minutes: 17 * 60 + 30,
            lunch_break_enabled: false,
            lunch_start_minutes: 12 * 60,
            lunch_end_minutes: 13 * 60,
            theme: ThemeMode::System,
            language: Language::Vietnamese,
            minimal_mode: false,
            full_screen_reminders: false,
            focus_mode_enabled: false,
            focus_countdown_seconds: 20,
            overlay_style: OverlayStyle::Modern,
            overlay_accent: OverlayAccent::Blue,
            idle_trim_seconds: DEFAULT_IDLE_TRIM_SECONDS,
            aumid: AUMID.to_owned(),
            sound: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Language {
    #[default]
    Vietnamese,
    English,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Gender {
    Female,
    Male,
    #[default]
    Other,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum WaterUnit {
    #[default]
    Milliliter,
    Ounce,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum OverlayStyle {
    #[default]
    Modern,
    Minimal,
    Bold,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum OverlayAccent {
    #[default]
    Blue,
    Green,
    Amber,
}

#[derive(Clone, Copy, Debug)]
pub enum ActivityKind {
    Water { ml: u64 },
    EyeRest,
    Movement,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct AppStats {
    pub days: Vec<DailyStats>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DailyStats {
    pub date: String,
    pub water_ml: u64,
    pub water_glasses: u64,
    pub eye_rest_completed: u64,
    pub movement_completed: u64,
}

#[derive(Debug)]
pub struct SingleInstanceGuard {
    mutex_handle: HANDLE,
    activation_event: HANDLE,
}

impl SingleInstanceGuard {
    pub fn activation_event_raw(&self) -> isize {
        self.activation_event.0
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.activation_event);
            let _ = CloseHandle(self.mutex_handle);
        }
    }
}

#[derive(Debug)]
pub struct AppPaths {
    pub config_path: PathBuf,
    pub stats_path: PathBuf,
    pub log_dir: PathBuf,
}

pub fn setup_process() -> Result<SingleInstanceGuard> {
    let guard = enforce_single_instance()?;
    init_logger()?;
    install_panic_hook();
    if let Err(error) = set_process_aumid(AUMID) {
        log::debug!("SetCurrentProcessExplicitAppUserModelID failed: {error:#}");
    }
    if let Err(error) = enable_efficiency_mode() {
        log::debug!("EcoQoS setup failed: {error:#}");
    }
    Ok(guard)
}

pub fn launch_minimized_arg() -> bool {
    env::args().any(|arg| {
        arg.eq_ignore_ascii_case("--minimized") || arg.eq_ignore_ascii_case("/minimized")
    })
}

pub fn resolve_paths() -> Result<AppPaths> {
    let exe_dir = env::current_exe()
        .context("cannot resolve current executable")?
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| anyhow!("current executable has no parent directory"))?;
    let local_base = local_data_dir(&exe_dir);
    let portable_config = exe_dir.join(CONFIG_FILE_NAME);
    let portable_stats = exe_dir.join(STATS_FILE_NAME);
    let data_base = if data_files_are_writable(&exe_dir, &[&portable_config, &portable_stats]) {
        exe_dir.clone()
    } else {
        local_base.clone()
    };

    Ok(AppPaths {
        config_path: data_base.join(CONFIG_FILE_NAME),
        stats_path: data_base.join(STATS_FILE_NAME),
        log_dir: local_base.join("logs"),
    })
}

fn local_data_dir(exe_dir: &Path) -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| exe_dir.to_path_buf())
        .join(APP_NAME)
}

fn data_files_are_writable(dir: &Path, files: &[&Path]) -> bool {
    if !dir.is_dir() {
        return false;
    }

    for file in files {
        if file.exists()
            && (!file.is_file() || fs::OpenOptions::new().write(true).open(file).is_err())
        {
            return false;
        }
    }

    let probe = dir.join(unique_temp_file_name("write-check"));
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

pub fn ensure_data_files(paths: &AppPaths) -> Result<()> {
    if !paths
        .config_path
        .try_exists()
        .with_context(|| format!("cannot check config {}", paths.config_path.display()))?
    {
        save_config(&paths.config_path, &AppConfig::default())?;
    }

    if !paths
        .stats_path
        .try_exists()
        .with_context(|| format!("cannot check stats {}", paths.stats_path.display()))?
    {
        save_stats(&paths.stats_path, &AppStats::default())?;
    }

    Ok(())
}

pub fn init_logger() -> Result<()> {
    let paths = resolve_paths()?;
    fs::create_dir_all(&paths.log_dir).ok();

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

pub fn install_panic_hook() {
    panic::set_hook(Box::new(|info| {
        log::error!("panic: {info}");
    }));
}

pub fn load_config(path: &Path) -> AppConfig {
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
    config.eye_countdown_seconds = config.eye_countdown_seconds.clamp(10, 60);
    config.movement_snooze_minutes = match config.movement_snooze_minutes {
        5 | 10 | 15 => config.movement_snooze_minutes,
        value if value < 8 => 5,
        value if value < 13 => 10,
        _ => 15,
    };
    config.work_start_minutes = config.work_start_minutes.min(1439);
    config.work_end_minutes = config.work_end_minutes.min(1439);
    config.lunch_start_minutes = config.lunch_start_minutes.min(1439);
    config.lunch_end_minutes = config.lunch_end_minutes.min(1439);
    config.focus_countdown_seconds = config.focus_countdown_seconds.clamp(5, 300);
    config.idle_trim_seconds = DEFAULT_IDLE_TRIM_SECONDS;
    config.aumid = AUMID.to_owned();
    config
}

fn clamp_stepped(value: u64, min: u64, max: u64, step: u64) -> u64 {
    let value = value.clamp(min, max);
    let step = step.max(1);
    (((value + step / 2) / step) * step).clamp(min, max)
}

pub fn save_config(path: &Path, config: &AppConfig) -> Result<()> {
    let config = normalize_config(config.clone());
    let raw = serde_json::to_string_pretty(&config).context("cannot serialize app config")?;
    write_text_atomic(path, &raw).with_context(|| format!("cannot write config {}", path.display()))
}

pub fn load_stats(path: &Path) -> AppStats {
    let stats = match read_text_file_limited(path, MAX_STATS_BYTES) {
        Ok(Some(raw)) => serde_json::from_str(&raw).unwrap_or_else(|error| {
            log::warn!("invalid stats at {}: {error}", path.display());
            AppStats::default()
        }),
        Ok(None) => AppStats::default(),
        Err(error) => {
            log::warn!("cannot read stats at {}: {error:#}", path.display());
            AppStats::default()
        }
    };
    normalize_stats(stats)
}

pub fn save_stats(path: &Path, stats: &AppStats) -> Result<()> {
    let stats = normalize_stats(stats.clone());
    let raw = serde_json::to_string_pretty(&stats).context("cannot serialize app stats")?;
    write_text_atomic(path, &raw).with_context(|| format!("cannot write stats {}", path.display()))
}

pub fn record_activity(path: &Path, activity: ActivityKind) -> Result<DailyStats> {
    let mut stats = load_stats(path);
    let today = today_date_string();
    let index = stats
        .days
        .iter()
        .position(|day| day.date == today)
        .unwrap_or_else(|| {
            stats.days.push(DailyStats {
                date: today.clone(),
                ..DailyStats::default()
            });
            stats.days.len() - 1
        });

    match activity {
        ActivityKind::Water { ml } => {
            stats.days[index].water_ml = stats.days[index].water_ml.saturating_add(ml);
            stats.days[index].water_glasses = stats.days[index].water_glasses.saturating_add(1);
        }
        ActivityKind::EyeRest => {
            stats.days[index].eye_rest_completed =
                stats.days[index].eye_rest_completed.saturating_add(1);
        }
        ActivityKind::Movement => {
            stats.days[index].movement_completed =
                stats.days[index].movement_completed.saturating_add(1);
        }
    }

    stats.days.sort_by(|left, right| left.date.cmp(&right.date));
    if stats.days.len() > 60 {
        let remove_count = stats.days.len() - 60;
        stats.days.drain(0..remove_count);
    }
    let today_stats = stats.days[index_for_today(&stats, &today)].clone();
    save_stats(path, &stats)?;
    Ok(today_stats)
}

pub fn today_stats(path: &Path) -> DailyStats {
    let today = today_date_string();
    load_stats(path)
        .days
        .into_iter()
        .find(|day| day.date == today)
        .unwrap_or(DailyStats {
            date: today,
            ..DailyStats::default()
        })
}

pub fn current_streak(stats: &AppStats) -> u64 {
    let mut date = Local::now().date_naive();
    let mut streak = 0;

    loop {
        let key = date.to_string();
        let Some(day) = stats.days.iter().find(|day| day.date == key) else {
            break;
        };
        if !day_has_activity(day) {
            break;
        }
        streak += 1;
        let Some(previous) = date.pred_opt() else {
            break;
        };
        date = previous;
    }

    streak
}

pub fn recent_daily_stats(stats: &AppStats, days: usize) -> Vec<DailyStats> {
    let today = Local::now().date_naive();
    (0..days)
        .rev()
        .map(|offset| {
            let date = today - ChronoDuration::days(offset as i64);
            let key = date.to_string();
            stats
                .days
                .iter()
                .find(|day| day.date == key)
                .cloned()
                .unwrap_or(DailyStats {
                    date: key,
                    ..DailyStats::default()
                })
        })
        .collect()
}

pub fn water_goal_ml(config: &AppConfig) -> u64 {
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

pub fn water_progress_percent(water_ml: u64, goal_ml: u64) -> u64 {
    if goal_ml == 0 {
        return 0;
    }
    ((water_ml.saturating_mul(100)) / goal_ml).min(999)
}

pub fn format_water_volume(ml: u64, unit: WaterUnit) -> String {
    match unit {
        WaterUnit::Milliliter => format!("{ml} ml"),
        WaterUnit::Ounce => format!("{:.1} oz", ml as f64 / 29.5735),
    }
}

pub fn today_date_string() -> String {
    Local::now().date_naive().to_string()
}

pub fn current_minutes_of_day() -> u16 {
    let now = Local::now();
    ((now.hour() * 60) + now.minute()) as u16
}

pub fn is_within_active_schedule(config: &AppConfig, minute: u16) -> bool {
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

pub fn minutes_until_active_schedule(config: &AppConfig, minute: u16) -> u64 {
    let minute = minute.min(1439);
    if is_within_active_schedule(config, minute) {
        return 0;
    }

    for offset in 1..=1440 {
        let candidate = ((minute as u64 + offset) % 1440) as u16;
        if is_within_active_schedule(config, candidate) {
            return offset;
        }
    }

    60
}

pub fn dark_mode_enabled(theme: ThemeMode) -> bool {
    match theme {
        ThemeMode::System => system_prefers_dark_mode(),
        ThemeMode::Light => false,
        ThemeMode::Dark => true,
    }
}

pub fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

pub fn update_url() -> &'static str {
    match option_env!("HEALTHY_REMINDERS_UPDATE_URL").map(str::trim) {
        Some(value) if is_https_url(value) => value,
        _ => GITHUB_RELEASES_URL,
    }
}

pub fn about_text(config_path: &Path, stats_path: &Path) -> String {
    format!(
        "{APP_NAME} {APP_VERSION} ({profile})\r\n\
         {APP_DESCRIPTION}\r\n\r\n\
         Build: {profile}\r\n\
         Config: {config}\r\n\
         Stats: {stats}\r\n\
         Updates: {updates}",
        profile = build_profile(),
        config = config_path.display(),
        stats = stats_path.display(),
        updates = update_url()
    )
}

pub fn movement_suggestion(config: &AppConfig) -> &'static str {
    if !config.movement_random_suggestion {
        return match config.language {
            Language::Vietnamese => "Rời ghế, giãn cơ nhẹ hoặc đi lại một chút.",
            Language::English => "Leave your chair, stretch lightly, or walk for a moment.",
        };
    }

    const VI_SUGGESTIONS: &[&str] = &[
        "Giãn lưng nhẹ trong 30 giây.",
        "Xoay cổ chậm 5 vòng mỗi bên.",
        "Đi bộ quanh phòng trong 1 phút.",
        "Duỗi vai và thả lỏng cánh tay.",
        "Đứng lên, hít sâu và vươn người.",
    ];
    const EN_SUGGESTIONS: &[&str] = &[
        "Stretch your back gently for 30 seconds.",
        "Do 5 slow neck rolls on each side.",
        "Walk around the room for 1 minute.",
        "Stretch your shoulders and relax your arms.",
        "Stand up, breathe deeply, and reach overhead.",
    ];
    let suggestions = match config.language {
        Language::Vietnamese => VI_SUGGESTIONS,
        Language::English => EN_SUGGESTIONS,
    };

    let index = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| (duration.as_secs() as usize) % suggestions.len())
        .unwrap_or(0);
    suggestions[index]
}

pub fn spawn_config_watcher(
    config_path: PathBuf,
    tx: Sender<crate::scheduler::SchedulerCommand>,
) -> Result<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name("ConfigFileWatcher".to_owned())
        .spawn(move || {
            let (watch_tx, watch_rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
            let mut watcher = match recommended_watcher(move |event| {
                let _ = watch_tx.send(event);
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    log::warn!("cannot create file watcher: {error}");
                    return;
                }
            };

            let watch_root = config_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            if let Err(error) = watcher.watch(&watch_root, RecursiveMode::NonRecursive) {
                log::warn!("cannot watch {}: {error}", watch_root.display());
                return;
            }

            while let Ok(event) = watch_rx.recv() {
                match event {
                    Ok(event)
                        if event
                            .paths
                            .iter()
                            .any(|path| same_path_loose(path, &config_path)) =>
                    {
                        let config = load_config(&config_path);
                        if tx
                            .send(crate::scheduler::SchedulerCommand::Reload(config))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => log::warn!("config watcher error: {error}"),
                }
            }
        })
        .context("failed to spawn ConfigFileWatcher")
}

pub fn enforce_single_instance() -> Result<SingleInstanceGuard> {
    let name = wide_null(MUTEX_NAME);
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) }
        .context("CreateMutexW failed")?;

    let already_exists = unsafe { GetLastError() }
        .err()
        .map(|error| error.code() == ERROR_ALREADY_EXISTS.to_hresult())
        .unwrap_or(false);
    if already_exists {
        signal_current_instance();
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(anyhow!("{APP_NAME} is already running"));
    }

    let activation_name = wide_null(ACTIVATION_EVENT_NAME);
    let activation_event =
        match unsafe { CreateEventW(None, false, false, PCWSTR(activation_name.as_ptr())) } {
            Ok(event) => event,
            Err(error) => {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Err(error).context("CreateEventW activation event failed");
            }
        };

    Ok(SingleInstanceGuard {
        mutex_handle: handle,
        activation_event,
    })
}

fn signal_current_instance() {
    if launch_minimized_arg() {
        return;
    }

    signal_activation_event(ACTIVATION_EVENT_NAME);
}

fn signal_activation_event(event_name: &str) {
    let name = wide_null(event_name);
    if let Ok(event) = unsafe { OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(name.as_ptr())) } {
        unsafe {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
        }
    }
}

pub fn init_com_runtime() -> Result<()> {
    match unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == RPC_E_CHANGED_MODE => Ok(()),
        Err(error) => Err(error).context("CoInitializeEx MTA failed"),
    }
}

struct ComApartment {
    initialized: bool,
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

fn init_com_sta_runtime() -> Result<ComApartment> {
    match unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) } {
        Ok(()) => Ok(ComApartment { initialized: true }),
        Err(error) if error.code() == RPC_E_CHANGED_MODE => Ok(ComApartment { initialized: false }),
        Err(error) => Err(error).context("CoInitializeEx STA failed"),
    }
}

pub fn set_process_aumid(aumid: &str) -> Result<()> {
    let aumid = wide_null(aumid);
    unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(aumid.as_ptr())) }
        .context("SetCurrentProcessExplicitAppUserModelID failed")
}

pub fn spawn_toast_shortcut_setup(aumid: String) -> Result<()> {
    thread::Builder::new()
        .name("ToastShortcutSetup".to_owned())
        .spawn(move || {
            if let Err(error) = ensure_toast_shortcut(&aumid) {
                log::warn!("cannot create toast shortcut: {error:#}");
            }
        })
        .context("failed to spawn ToastShortcutSetup")?;
    Ok(())
}

pub fn ensure_toast_shortcut(aumid: &str) -> Result<()> {
    let _com = init_com_sta_runtime()?;

    let shortcut_path = toast_shortcut_path()?;
    if let Some(parent) = shortcut_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create shortcut directory {}", parent.display()))?;
    }

    let exe = env::current_exe().context("cannot resolve current executable")?;
    let exe_wide = wide_os_null(exe.as_os_str());
    let args_wide = wide_null("--minimized");
    let aumid_wide = wide_null(aumid);
    let shortcut_wide = wide_os_null(shortcut_path.as_os_str());

    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .context("CoCreateInstance(ShellLink) failed")?;
        link.SetPath(PCWSTR(exe_wide.as_ptr()))
            .context("IShellLinkW::SetPath failed")?;
        link.SetArguments(PCWSTR(args_wide.as_ptr()))
            .context("IShellLinkW::SetArguments failed")?;

        let store: IPropertyStore = link
            .cast()
            .context("ShellLink cast to IPropertyStore failed")?;
        let prop = propvariant_from_wide_string(&aumid_wide);
        store
            .SetValue(&PKEY_APP_USER_MODEL_ID, &prop)
            .context("IPropertyStore::SetValue AppUserModelID failed")?;
        store.Commit().context("IPropertyStore::Commit failed")?;

        let persist: IPersistFile = link
            .cast()
            .context("ShellLink cast to IPersistFile failed")?;
        persist
            .Save(PCWSTR(shortcut_wide.as_ptr()), BOOL(1))
            .context("IPersistFile::Save toast shortcut failed")?;
    }

    Ok(())
}

fn propvariant_from_wide_string(value: &[u16]) -> PROPVARIANT {
    PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: PWSTR(value.as_ptr() as *mut u16),
                },
            }),
        },
    }
}

pub fn enable_efficiency_mode() -> Result<()> {
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    };

    unsafe {
        SetProcessInformation(
            GetCurrentProcess(),
            ProcessPowerThrottling,
            &state as *const _ as *const _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        )
    }
    .context("SetProcessInformation(ProcessPowerThrottling) failed")
}

pub fn trim_memory() {
    if let Err(error) = unsafe { EmptyWorkingSet(GetCurrentProcess()) } {
        log::debug!("EmptyWorkingSet failed: {error}");
    }
}

pub fn system_prefers_dark_mode() -> bool {
    read_hkcu_dword(PERSONALIZE_KEY, APPS_USE_LIGHT_THEME)
        .map(|value| value == 0)
        .unwrap_or(false)
}

pub fn autostart_is_enabled() -> Result<bool> {
    let key = open_run_key(KEY_QUERY_VALUE)?;
    let name = wide_null(AUTOSTART_VALUE_NAME);
    let mut size = 0u32;
    let result = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            None,
            None,
            Some(&mut size),
        )
    };
    drop(key);
    Ok(result.is_ok())
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    let key = open_run_key(KEY_SET_VALUE)?;
    let name = wide_null(AUTOSTART_VALUE_NAME);

    if enabled {
        let exe = env::current_exe().context("cannot resolve current executable")?;
        let wide = autostart_command_wide_null(exe.as_os_str());
        let bytes = wide_as_bytes(&wide);
        unsafe { RegSetValueExW(key.0, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes)) }
            .context("RegSetValueExW autostart failed")?;
    } else if let Err(error) = unsafe { RegDeleteValueW(key.0, PCWSTR(name.as_ptr())) } {
        log::debug!("RegDeleteValueW autostart failed: {error}");
    }

    Ok(())
}

pub fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

pub fn wide_os_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

pub fn quoted_path_wide_null(value: &OsStr) -> Vec<u16> {
    let mut wide = Vec::with_capacity(value.len() + 3);
    wide.push(b'"' as u16);
    wide.extend(value.encode_wide());
    wide.push(b'"' as u16);
    wide.push(0);
    wide
}

pub fn autostart_command_wide_null(value: &OsStr) -> Vec<u16> {
    let mut wide = quoted_path_wide_null(value);
    wide.pop();
    wide.push(b' ' as u16);
    wide.extend(OsStr::new("--minimized").encode_wide());
    wide.push(0);
    wide
}

pub fn debounce_trim() {
    thread::spawn(|| {
        thread::sleep(Duration::from_secs(2));
        trim_memory();
    });
}

fn open_run_key(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Result<RegKey> {
    let path = wide_null(RUN_KEY);
    let mut key = HKEY::default();
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            0,
            access,
            &mut key,
        )
    }
    .context("RegOpenKeyExW HKCU Run failed")?;
    Ok(RegKey(key))
}

fn read_hkcu_dword(path: &str, name: &str) -> Result<u32> {
    let path = wide_null(path);
    let name = wide_null(name);
    let mut key = HKEY::default();
    unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(path.as_ptr()),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    }
    .context("RegOpenKeyExW HKCU failed")?;
    let key = RegKey(key);

    let mut value_type = REG_VALUE_TYPE(0);
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut value_type),
            Some(&mut value as *mut u32 as *mut u8),
            Some(&mut size),
        )
    }
    .context("RegQueryValueExW DWORD failed")?;

    if value_type.0 == REG_DWORD.0 && size == std::mem::size_of::<u32>() as u32 {
        Ok(value)
    } else {
        Err(anyhow!("registry value is not REG_DWORD"))
    }
}

fn toast_shortcut_path() -> Result<PathBuf> {
    let appdata = env::var_os("APPDATA").ok_or_else(|| anyhow!("APPDATA is not set"))?;
    Ok(PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs")
        .join(format!("{APP_NAME}.lnk")))
}

fn wide_as_bytes(value: &[u16]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(value.as_ptr() as *const u8, value.len() * 2) }
}

fn read_text_file_limited(path: &Path, max_bytes: u64) -> Result<Option<String>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot stat {}", path.display()));
        }
    };

    if !metadata.is_file() {
        return Err(anyhow!("{} is not a regular file", path.display()));
    }
    if metadata.len() > max_bytes {
        return Err(anyhow!(
            "{} is too large: {} bytes, max {} bytes",
            path.display(),
            metadata.len(),
            max_bytes
        ));
    }

    fs::read_to_string(path)
        .map(Some)
        .with_context(|| format!("cannot read {}", path.display()))
}

fn write_text_atomic(path: &Path, raw: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("cannot create data directory {}", parent.display()))?;

    let temp_path = create_temp_sibling(path, raw)?;
    let result = replace_file(&temp_path, path);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn create_temp_sibling(path: &Path, raw: &str) -> Result<PathBuf> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("{} has no parent directory", path.display()))?;

    for attempt in 0..16 {
        let temp_path = parent.join(unique_temp_file_name(&format!(
            "{}.{attempt}",
            path.file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("data-json")
        )));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
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

fn replace_file(temp_path: &Path, target_path: &Path) -> Result<()> {
    let temp = wide_os_null(temp_path.as_os_str());
    let target = wide_os_null(target_path.as_os_str());
    unsafe {
        MoveFileExW(
            PCWSTR(temp.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .ok()
    .with_context(|| {
        format!(
            "cannot replace {} with {}",
            target_path.display(),
            temp_path.display()
        )
    })
}

fn unique_temp_file_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(".{APP_NAME}.{prefix}.{}.{}.tmp", process::id(), nanos)
}

fn normalize_stats(mut stats: AppStats) -> AppStats {
    stats
        .days
        .retain(|day| NaiveDate::parse_from_str(&day.date, "%Y-%m-%d").is_ok());
    for day in &mut stats.days {
        day.water_ml = day.water_ml.min(MAX_DAILY_WATER_ML);
        day.water_glasses = day.water_glasses.min(MAX_DAILY_COUNT);
        day.eye_rest_completed = day.eye_rest_completed.min(MAX_DAILY_COUNT);
        day.movement_completed = day.movement_completed.min(MAX_DAILY_COUNT);
    }

    stats.days.sort_by(|left, right| left.date.cmp(&right.date));
    let mut merged = Vec::<DailyStats>::with_capacity(stats.days.len().min(60));
    for day in stats.days {
        if let Some(last) = merged.last_mut() {
            if last.date == day.date {
                last.water_ml = last
                    .water_ml
                    .saturating_add(day.water_ml)
                    .min(MAX_DAILY_WATER_ML);
                last.water_glasses = last
                    .water_glasses
                    .saturating_add(day.water_glasses)
                    .min(MAX_DAILY_COUNT);
                last.eye_rest_completed = last
                    .eye_rest_completed
                    .saturating_add(day.eye_rest_completed)
                    .min(MAX_DAILY_COUNT);
                last.movement_completed = last
                    .movement_completed
                    .saturating_add(day.movement_completed)
                    .min(MAX_DAILY_COUNT);
                continue;
            }
        }
        merged.push(day);
    }

    if merged.len() > 60 {
        let remove_count = merged.len() - 60;
        merged.drain(0..remove_count);
    }
    stats.days = merged;
    stats
}

fn is_https_url(value: &str) -> bool {
    value.starts_with("https://")
}

fn same_path_loose(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.file_name(), right.file_name()) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

fn index_for_today(stats: &AppStats, today: &str) -> usize {
    stats
        .days
        .iter()
        .position(|day| day.date == today)
        .unwrap_or_else(|| stats.days.len().saturating_sub(1))
}

fn day_has_activity(day: &DailyStats) -> bool {
    day.water_ml > 0 || day.eye_rest_completed > 0 || day.movement_completed > 0
}

fn minute_in_range(minute: u16, start: u16, end: u16) -> bool {
    if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    }
}

fn round_to_step(value: u64, step: u64) -> u64 {
    let step = step.max(1);
    ((value + (step / 2)) / step) * step
}

struct RegKey(HKEY);

impl Drop for RegKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}
