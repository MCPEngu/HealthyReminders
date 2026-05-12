use std::path::Path;

use crate::{core, scheduler::ReminderKind};

#[derive(Clone, Copy)]
pub(super) struct SettingsText {
    pub(super) window_title: &'static str,
    pub(super) section_today: &'static str,
    pub(super) section_reminders: &'static str,
    pub(super) section_water: &'static str,
    pub(super) section_schedule: &'static str,
    pub(super) section_display: &'static str,
    pub(super) section_app: &'static str,
    pub(super) water_enabled: &'static str,
    pub(super) eye_enabled: &'static str,
    pub(super) movement_enabled: &'static str,
    pub(super) auto_goal: &'static str,
    pub(super) weight: &'static str,
    pub(super) gender: &'static str,
    pub(super) unit: &'static str,
    pub(super) manual_goal: &'static str,
    pub(super) log_water: &'static str,
    pub(super) water_minutes: &'static str,
    pub(super) eye_minutes: &'static str,
    pub(super) stand_minutes: &'static str,
    pub(super) stand_duration: &'static str,
    pub(super) default_glass: &'static str,
    pub(super) eye_countdown: &'static str,
    pub(super) eye_silent: &'static str,
    pub(super) movement_random: &'static str,
    pub(super) snooze: &'static str,
    pub(super) work_enabled: &'static str,
    pub(super) work_hours: &'static str,
    pub(super) lunch_enabled: &'static str,
    pub(super) lunch_hours: &'static str,
    pub(super) sound: &'static str,
    pub(super) autostart: &'static str,
    pub(super) minimal_mode: &'static str,
    pub(super) theme: &'static str,
    pub(super) language: &'static str,
    pub(super) full_screen: &'static str,
    pub(super) focus_mode: &'static str,
    pub(super) focus_seconds: &'static str,
    pub(super) overlay_style: &'static str,
    pub(super) overlay_accent: &'static str,
    pub(super) save: &'static str,
    pub(super) test_water: &'static str,
    pub(super) test_eyes: &'static str,
    pub(super) test_stand: &'static str,
    pub(super) test_work: &'static str,
    pub(super) about: &'static str,
    pub(super) updates: &'static str,
    pub(super) hide: &'static str,
    pub(super) theme_items: [&'static str; 3],
    pub(super) gender_items: [&'static str; 3],
    pub(super) language_items: [&'static str; 2],
    pub(super) overlay_style_items: [&'static str; 3],
    pub(super) overlay_accent_items: [&'static str; 3],
    pub(super) water_chart_title_ml: &'static str,
    pub(super) water_chart_title_oz: &'static str,
    pub(super) sending_test: &'static str,
    pub(super) toast_failed: &'static str,
    pub(super) notifier_timeout: &'static str,
    pub(super) notifier_stopped: &'static str,
    pub(super) saved: &'static str,
    pub(super) save_failed: &'static str,
    pub(super) updates_opened: &'static str,
}

pub(super) fn settings_text(language: core::Language) -> SettingsText {
    match language {
        core::Language::Vietnamese => SettingsText {
            window_title: "HealthyReminders - Cài đặt",
            section_today: "Hôm nay",
            section_reminders: "Nhắc nhở",
            section_water: "Nước",
            section_schedule: "Lịch",
            section_display: "Màn hình",
            section_app: "Ứng dụng",
            water_enabled: "Nước",
            eye_enabled: "Nghỉ mắt",
            movement_enabled: "Đứng dậy",
            auto_goal: "Tự tính mục tiêu nước",
            weight: "Cân nặng (kg)",
            gender: "Giới tính",
            unit: "Đơn vị",
            manual_goal: "Mục tiêu mỗi ngày",
            log_water: "Ghi 1 ly nước",
            water_minutes: "Uống nước mỗi (phút)",
            eye_minutes: "Nghỉ mắt mỗi (phút)",
            stand_minutes: "Đứng dậy mỗi (phút)",
            stand_duration: "Thời lượng đứng (phút)",
            default_glass: "Dung tích mỗi ly (ml)",
            eye_countdown: "Đếm ngược nghỉ mắt (giây)",
            eye_silent: "Nghỉ mắt im lặng",
            movement_random: "Gợi ý vận động",
            snooze: "Tạm hoãn khi họp (phút)",
            work_enabled: "Chỉ nhắc trong giờ làm",
            work_hours: "Giờ làm (HH:MM)",
            lunch_enabled: "Không nhắc khi nghỉ trưa",
            lunch_hours: "Nghỉ trưa (HH:MM)",
            sound: "Âm thanh thông báo",
            autostart: "Tự chạy cùng Windows",
            minimal_mode: "Tối giản",
            theme: "Nền",
            language: "Ngôn ngữ",
            full_screen: "Nhắc toàn màn hình",
            focus_mode: "Đếm ngược bắt buộc",
            focus_seconds: "Thời gian đếm ngược (giây)",
            overlay_style: "Kiểu overlay",
            overlay_accent: "Màu nhấn",
            save: "Lưu",
            test_water: "Test nước",
            test_eyes: "Test nghỉ mắt",
            test_stand: "Test đứng dậy",
            test_work: "Test làm việc",
            about: "Thông tin",
            updates: "Cập nhật",
            hide: "Ẩn xuống tray",
            theme_items: ["Theo hệ thống", "Sáng", "Tối"],
            gender_items: ["Khác", "Nam", "Nữ"],
            language_items: ["Tiếng Việt", "English"],
            overlay_style_items: ["Hiện đại", "Tối giản", "Nổi bật"],
            overlay_accent_items: ["Xanh dương", "Xanh lá", "Vàng cam"],
            water_chart_title_ml: "7 ngày gần đây (ngày/tháng):",
            water_chart_title_oz: "7 ngày gần đây (ngày/tháng):",
            sending_test: "Đang gửi thông báo thử...",
            toast_failed: "Không gửi được toast. Xem hộp thoại lỗi.",
            notifier_timeout: "Notifier không phản hồi trong 3 giây.",
            notifier_stopped: "Notifier đã dừng.",
            saved: "Đã lưu cài đặt.",
            save_failed: "Không lưu được cài đặt.",
            updates_opened: "Đã mở trang cập nhật.",
        },
        core::Language::English => SettingsText {
            window_title: "HealthyReminders - Settings",
            section_today: "Today",
            section_reminders: "Reminders",
            section_water: "Water",
            section_schedule: "Schedule",
            section_display: "Display",
            section_app: "App",
            water_enabled: "Water",
            eye_enabled: "Eye rest",
            movement_enabled: "Stand up",
            auto_goal: "Auto daily goal",
            weight: "Weight (kg)",
            gender: "Gender",
            unit: "Unit",
            manual_goal: "Manual goal",
            log_water: "Log glass",
            water_minutes: "Water every (min)",
            eye_minutes: "Eye rest every (min)",
            stand_minutes: "Stand every (min)",
            stand_duration: "Stand duration (min)",
            default_glass: "Glass size (ml)",
            eye_countdown: "Eye countdown (sec)",
            eye_silent: "Silent eye rest",
            movement_random: "Movement tips",
            snooze: "Snooze (min)",
            work_enabled: "Only during work hours",
            work_hours: "Work time (HH:MM)",
            lunch_enabled: "Pause during lunch",
            lunch_hours: "Lunch time (HH:MM)",
            sound: "Notification sound",
            autostart: "Start with Windows",
            minimal_mode: "Minimal",
            theme: "Background",
            language: "Language",
            full_screen: "Full-screen reminder",
            focus_mode: "Required countdown",
            focus_seconds: "Countdown time (sec)",
            overlay_style: "Overlay style",
            overlay_accent: "Accent",
            save: "Save",
            test_water: "Test water",
            test_eyes: "Test eye rest",
            test_stand: "Test stand up",
            test_work: "Test work",
            about: "About",
            updates: "Updates",
            hide: "Hide to tray",
            theme_items: ["System", "Light", "Dark"],
            gender_items: ["Other", "Male", "Female"],
            language_items: ["Vietnamese", "English"],
            overlay_style_items: ["Modern", "Minimal", "Bold"],
            overlay_accent_items: ["Blue", "Green", "Amber"],
            water_chart_title_ml: "Last 7 days (day/month):",
            water_chart_title_oz: "Last 7 days (day/month):",
            sending_test: "Sending test notification...",
            toast_failed: "Toast failed. See the error dialog.",
            notifier_timeout: "Notifier did not respond within 3 seconds.",
            notifier_stopped: "Notifier has stopped.",
            saved: "Settings saved.",
            save_failed: "Could not save settings.",
            updates_opened: "Opened the updates page.",
        },
    }
}

pub(super) enum SettingsMenuTitle {
    View,
    Tools,
    Settings,
    Help,
}

pub(super) fn settings_menu_title(
    language: core::Language,
    title: SettingsMenuTitle,
) -> &'static str {
    match (language, title) {
        (core::Language::Vietnamese, SettingsMenuTitle::View) => "Xem",
        (core::Language::Vietnamese, SettingsMenuTitle::Tools) => "Công cụ",
        (core::Language::Vietnamese, SettingsMenuTitle::Settings) => "Cài đặt",
        (core::Language::Vietnamese, SettingsMenuTitle::Help) => "Trợ giúp",
        (core::Language::English, SettingsMenuTitle::View) => "View",
        (core::Language::English, SettingsMenuTitle::Tools) => "Tools",
        (core::Language::English, SettingsMenuTitle::Settings) => "Settings",
        (core::Language::English, SettingsMenuTitle::Help) => "Help",
    }
}

pub(super) fn about_title(language: core::Language) -> &'static str {
    match language {
        core::Language::Vietnamese => "Thông tin",
        core::Language::English => "About",
    }
}

pub(super) fn about_text(
    language: core::Language,
    config_path: &Path,
    stats_path: &Path,
) -> String {
    match language {
        core::Language::Vietnamese => {
            format!(
                "{} {} ({profile})\r\n\
                 Ứng dụng nhắc uống nước, nghỉ mắt và vận động.\r\n\r\n\
                 Bản dựng: {profile}\r\n\
                 Cấu hình: {config}\r\n\
                 Thống kê: {stats}\r\n\
                 Cập nhật: {updates}",
                core::APP_NAME,
                core::APP_VERSION,
                profile = core::build_profile(),
                config = config_path.display(),
                stats = stats_path.display(),
                updates = core::update_url()
            )
        }
        core::Language::English => core::about_text(config_path, stats_path),
    }
}

pub(super) fn glass_label(language: core::Language) -> &'static str {
    match language {
        core::Language::Vietnamese => "ly",
        core::Language::English => "glasses",
    }
}

pub(super) fn water_logged_text(language: core::Language, amount: &str) -> String {
    match language {
        core::Language::Vietnamese => format!("Đã ghi {amount}."),
        core::Language::English => format!("Logged {amount}."),
    }
}

pub(super) fn water_chart_text(stats: &core::AppStats, config: &core::AppConfig) -> String {
    let text = settings_text(config.language);
    let title = match config.water_unit {
        core::WaterUnit::Milliliter => text.water_chart_title_ml,
        core::WaterUnit::Ounce => text.water_chart_title_oz,
    };
    let days = core::recent_daily_stats(stats, 7);
    let mut lines = vec![title.to_owned()];
    for day in days {
        let date = date_dd_mm(&day.date);
        lines.push(format!(
            "{date}: {}",
            core::format_water_volume(day.water_ml, config.water_unit)
        ));
    }
    lines.join("\r\n")
}

pub(super) fn test_success_text(kind: ReminderKind, language: core::Language) -> &'static str {
    match (language, kind) {
        (core::Language::Vietnamese, ReminderKind::Water) => "Đã gửi thông báo thử uống nước.",
        (core::Language::Vietnamese, ReminderKind::Eyes) => "Đã gửi thông báo thử nghỉ mắt.",
        (core::Language::Vietnamese, ReminderKind::Stand) => "Đã gửi thông báo thử đứng dậy.",
        (core::Language::Vietnamese, ReminderKind::Work) => {
            "Đã gửi thông báo thử quay lại làm việc."
        }
        (core::Language::English, ReminderKind::Water) => "Sent the water test reminder.",
        (core::Language::English, ReminderKind::Eyes) => "Sent the eye-rest test reminder.",
        (core::Language::English, ReminderKind::Stand) => "Sent the stand-up test reminder.",
        (core::Language::English, ReminderKind::Work) => "Sent the back-to-work test reminder.",
    }
}

fn date_dd_mm(date: &str) -> String {
    let mut parts = date.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(_year), Some(month), Some(day)) if day.len() == 2 && month.len() == 2 => {
            format!("{day}/{month}")
        }
        _ => date.to_owned(),
    }
}
