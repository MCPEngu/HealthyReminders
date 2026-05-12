use super::ids::*;

#[derive(Clone, Copy)]
pub(super) enum SettingsPage {
    Reminders = 0,
    Water = 1,
    Schedule = 2,
    Display = 3,
    App = 4,
}

const REMINDER_PAGE_IDS: &[i32] = &[
    ID_SECTION_REMINDERS,
    ID_WATER_ENABLED,
    ID_EYE_ENABLED,
    ID_MOVEMENT_ENABLED,
    ID_LABEL_WATER_MINUTES,
    ID_WATER_MINUTES,
    ID_LABEL_EYE_MINUTES,
    ID_EYE_MINUTES,
    ID_LABEL_STAND_MINUTES,
    ID_STAND_MINUTES,
    ID_LABEL_STAND_DURATION,
    ID_STAND_DURATION,
    ID_LABEL_EYE_COUNTDOWN,
    ID_EYE_COUNTDOWN,
    ID_EYE_SILENT,
    ID_MOVEMENT_RANDOM,
];

const WATER_PAGE_IDS: &[i32] = &[
    ID_SECTION_WATER,
    ID_WATER_GOAL_AUTO,
    ID_LOG_WATER,
    ID_LABEL_WEIGHT,
    ID_WATER_WEIGHT,
    ID_LABEL_GENDER,
    ID_WATER_GENDER,
    ID_LABEL_UNIT,
    ID_WATER_UNIT,
    ID_LABEL_GOAL_OVERRIDE,
    ID_WATER_GOAL_OVERRIDE,
    ID_LABEL_DEFAULT_GLASS,
    ID_DEFAULT_GLASS,
];

const SCHEDULE_PAGE_IDS: &[i32] = &[
    ID_SECTION_SCHEDULE,
    ID_WORK_ENABLED,
    ID_LUNCH_ENABLED,
    ID_LABEL_WORK_HOURS,
    ID_WORK_START,
    ID_LABEL_WORK_SEPARATOR,
    ID_WORK_END,
    ID_LABEL_LUNCH_HOURS,
    ID_LUNCH_START,
    ID_LABEL_LUNCH_SEPARATOR,
    ID_LUNCH_END,
];

const DISPLAY_PAGE_IDS: &[i32] = &[
    ID_SECTION_DISPLAY,
    ID_FULLSCREEN_REMINDERS,
    ID_FOCUS_MODE,
    ID_LABEL_FOCUS_SECONDS,
    ID_FOCUS_SECONDS,
    ID_LABEL_OVERLAY_STYLE,
    ID_OVERLAY_STYLE,
    ID_LABEL_SNOOZE,
    ID_MOVEMENT_SNOOZE,
    ID_LABEL_OVERLAY_ACCENT,
    ID_OVERLAY_ACCENT,
];

const APP_PAGE_IDS: &[i32] = &[
    ID_SECTION_APP,
    ID_SOUND,
    ID_AUTOSTART,
    ID_MINIMAL_MODE,
    ID_LABEL_THEME,
    ID_THEME,
    ID_LABEL_LANGUAGE,
    ID_LANGUAGE,
];

pub(super) const ALL_PAGE_IDS: &[i32] = &[
    ID_SECTION_REMINDERS,
    ID_WATER_ENABLED,
    ID_EYE_ENABLED,
    ID_MOVEMENT_ENABLED,
    ID_LABEL_WATER_MINUTES,
    ID_WATER_MINUTES,
    ID_LABEL_EYE_MINUTES,
    ID_EYE_MINUTES,
    ID_LABEL_STAND_MINUTES,
    ID_STAND_MINUTES,
    ID_LABEL_STAND_DURATION,
    ID_STAND_DURATION,
    ID_LABEL_EYE_COUNTDOWN,
    ID_EYE_COUNTDOWN,
    ID_EYE_SILENT,
    ID_MOVEMENT_RANDOM,
    ID_SECTION_WATER,
    ID_WATER_GOAL_AUTO,
    ID_LOG_WATER,
    ID_LABEL_WEIGHT,
    ID_WATER_WEIGHT,
    ID_LABEL_GENDER,
    ID_WATER_GENDER,
    ID_LABEL_UNIT,
    ID_WATER_UNIT,
    ID_LABEL_GOAL_OVERRIDE,
    ID_WATER_GOAL_OVERRIDE,
    ID_LABEL_DEFAULT_GLASS,
    ID_DEFAULT_GLASS,
    ID_SECTION_SCHEDULE,
    ID_WORK_ENABLED,
    ID_LUNCH_ENABLED,
    ID_LABEL_WORK_HOURS,
    ID_WORK_START,
    ID_LABEL_WORK_SEPARATOR,
    ID_WORK_END,
    ID_LABEL_LUNCH_HOURS,
    ID_LUNCH_START,
    ID_LABEL_LUNCH_SEPARATOR,
    ID_LUNCH_END,
    ID_SECTION_DISPLAY,
    ID_FULLSCREEN_REMINDERS,
    ID_FOCUS_MODE,
    ID_LABEL_FOCUS_SECONDS,
    ID_FOCUS_SECONDS,
    ID_LABEL_OVERLAY_STYLE,
    ID_OVERLAY_STYLE,
    ID_LABEL_SNOOZE,
    ID_MOVEMENT_SNOOZE,
    ID_LABEL_OVERLAY_ACCENT,
    ID_OVERLAY_ACCENT,
    ID_SECTION_APP,
    ID_SOUND,
    ID_AUTOSTART,
    ID_MINIMAL_MODE,
    ID_LABEL_THEME,
    ID_THEME,
    ID_LABEL_LANGUAGE,
    ID_LANGUAGE,
];

pub(super) fn page_control_ids(page: SettingsPage) -> &'static [i32] {
    match page {
        SettingsPage::Reminders => REMINDER_PAGE_IDS,
        SettingsPage::Water => WATER_PAGE_IDS,
        SettingsPage::Schedule => SCHEDULE_PAGE_IDS,
        SettingsPage::Display => DISPLAY_PAGE_IDS,
        SettingsPage::App => APP_PAGE_IDS,
    }
}

pub(super) fn page_from_nav_id(id: i32) -> Option<SettingsPage> {
    match id {
        ID_NAV_REMINDERS => Some(SettingsPage::Reminders),
        ID_NAV_WATER => Some(SettingsPage::Water),
        ID_NAV_SCHEDULE => Some(SettingsPage::Schedule),
        ID_NAV_DISPLAY => Some(SettingsPage::Display),
        ID_NAV_APP => Some(SettingsPage::App),
        _ => None,
    }
}

pub(super) fn page_from_index(index: usize) -> SettingsPage {
    match index {
        1 => SettingsPage::Water,
        2 => SettingsPage::Schedule,
        3 => SettingsPage::Display,
        4 => SettingsPage::App,
        _ => SettingsPage::Reminders,
    }
}
