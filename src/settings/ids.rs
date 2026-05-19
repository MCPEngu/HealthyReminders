pub(super) const CLASS_NAME: &str = "HealthyRemindersSettingsWindow";
pub(super) const APP_ICON_RESOURCE_ID: usize = 1;
pub(super) const SETTINGS_WINDOW_WIDTH: i32 = 760;
pub(super) const SETTINGS_WINDOW_HEIGHT: i32 = 740;

pub(super) const ID_WATER_MINUTES: i32 = 101;
pub(super) const ID_EYE_MINUTES: i32 = 102;
pub(super) const ID_SOUND: i32 = 104;
pub(super) const ID_AUTOSTART: i32 = 105;
pub(super) const ID_STAND_MINUTES: i32 = 106;
pub(super) const ID_STAND_DURATION: i32 = 107;
pub(super) const ID_WATER_ENABLED: i32 = 108;
pub(super) const ID_EYE_ENABLED: i32 = 109;
pub(super) const ID_MOVEMENT_ENABLED: i32 = 110;
pub(super) const ID_DEFAULT_GLASS: i32 = 111;
pub(super) const ID_WORK_ENABLED: i32 = 112;
pub(super) const ID_WORK_START: i32 = 113;
pub(super) const ID_WORK_END: i32 = 114;
pub(super) const ID_LUNCH_ENABLED: i32 = 115;
pub(super) const ID_LUNCH_START: i32 = 116;
pub(super) const ID_LUNCH_END: i32 = 117;
pub(super) const ID_THEME: i32 = 118;
pub(super) const ID_LANGUAGE: i32 = 119;
pub(super) const ID_MINIMAL_MODE: i32 = 120;
pub(super) const ID_FULLSCREEN_REMINDERS: i32 = 121;
pub(super) const ID_FOCUS_MODE: i32 = 122;
pub(super) const ID_FOCUS_SECONDS: i32 = 123;
pub(super) const ID_OVERLAY_STYLE: i32 = 124;
pub(super) const ID_OVERLAY_ACCENT: i32 = 125;
pub(super) const ID_WATER_GOAL_AUTO: i32 = 126;
pub(super) const ID_WATER_WEIGHT: i32 = 127;
pub(super) const ID_WATER_GENDER: i32 = 128;
pub(super) const ID_WATER_UNIT: i32 = 129;
pub(super) const ID_WATER_GOAL_OVERRIDE: i32 = 130;
pub(super) const ID_EYE_COUNTDOWN: i32 = 131;
pub(super) const ID_EYE_SILENT: i32 = 132;
pub(super) const ID_MOVEMENT_RANDOM: i32 = 133;
pub(super) const ID_MOVEMENT_SNOOZE: i32 = 134;

pub(super) const ID_SAVE: i32 = 201;
pub(super) const ID_TEST_WATER: i32 = 202;
pub(super) const ID_TEST_EYES: i32 = 203;
pub(super) const ID_HIDE: i32 = 204;
pub(super) const ID_TEST_STAND: i32 = 205;
pub(super) const ID_TEST_WORK: i32 = 206;
pub(super) const ID_LOG_WATER: i32 = 207;
pub(super) const ID_ABOUT: i32 = 208;
pub(super) const ID_CHECK_UPDATES: i32 = 209;

pub(super) const ID_STATUS: i32 = 301;
pub(super) const ID_DASHBOARD: i32 = 302;
pub(super) const ID_WATER_CHART: i32 = 303;

pub(super) const ID_LABEL_WATER_MINUTES: i32 = 401;
pub(super) const ID_LABEL_EYE_MINUTES: i32 = 402;
pub(super) const ID_LABEL_STAND_MINUTES: i32 = 403;
pub(super) const ID_LABEL_STAND_DURATION: i32 = 404;
pub(super) const ID_LABEL_DEFAULT_GLASS: i32 = 405;
pub(super) const ID_LABEL_EYE_COUNTDOWN: i32 = 407;
pub(super) const ID_LABEL_WEIGHT: i32 = 408;
pub(super) const ID_LABEL_GENDER: i32 = 409;
pub(super) const ID_LABEL_UNIT: i32 = 410;
pub(super) const ID_LABEL_GOAL_OVERRIDE: i32 = 411;
pub(super) const ID_LABEL_SNOOZE: i32 = 412;
pub(super) const ID_LABEL_WORK_HOURS: i32 = 413;
pub(super) const ID_LABEL_LUNCH_HOURS: i32 = 414;
pub(super) const ID_LABEL_THEME: i32 = 415;
pub(super) const ID_LABEL_LANGUAGE: i32 = 416;
pub(super) const ID_LABEL_FOCUS_SECONDS: i32 = 417;
pub(super) const ID_LABEL_OVERLAY_STYLE: i32 = 418;
pub(super) const ID_LABEL_OVERLAY_ACCENT: i32 = 419;
pub(super) const ID_SECTION_TODAY: i32 = 420;
pub(super) const ID_SECTION_REMINDERS: i32 = 421;
pub(super) const ID_SECTION_WATER: i32 = 422;
pub(super) const ID_SECTION_SCHEDULE: i32 = 423;
pub(super) const ID_SECTION_DISPLAY: i32 = 424;
pub(super) const ID_SECTION_APP: i32 = 425;
pub(super) const ID_LABEL_WORK_SEPARATOR: i32 = 427;
pub(super) const ID_LABEL_LUNCH_SEPARATOR: i32 = 428;

pub(super) const ID_NAV_REMINDERS: i32 = 501;
pub(super) const ID_NAV_WATER: i32 = 502;
pub(super) const ID_NAV_SCHEDULE: i32 = 503;
pub(super) const ID_NAV_DISPLAY: i32 = 504;
pub(super) const ID_NAV_APP: i32 = 505;

pub(super) const NAV_CONTROL_IDS: &[i32] = &[
    ID_NAV_REMINDERS,
    ID_NAV_WATER,
    ID_NAV_SCHEDULE,
    ID_NAV_DISPLAY,
    ID_NAV_APP,
];

pub(super) const CB_ADDSTRING: u32 = 0x0143;
pub(super) const CB_GETCURSEL: u32 = 0x0147;
pub(super) const CB_RESETCONTENT: u32 = 0x014B;
pub(super) const CB_SETCURSEL: u32 = 0x014E;
pub(super) const BN_CLICKED: u16 = 0;
pub(super) const CBN_SELCHANGE: u16 = 1;
pub(super) const CBS_DROPDOWNLIST_VALUE: i32 = 0x0003;
pub(super) const WM_UAHDRAWMENU: u32 = 0x0091;
pub(super) const WM_UAHDRAWMENUITEM: u32 = 0x0092;

pub(super) const CONTROL_IDS: &[i32] = &[
    ID_WATER_MINUTES,
    ID_EYE_MINUTES,
    ID_STAND_MINUTES,
    ID_STAND_DURATION,
    ID_WATER_ENABLED,
    ID_EYE_ENABLED,
    ID_MOVEMENT_ENABLED,
    ID_DEFAULT_GLASS,
    ID_WORK_ENABLED,
    ID_WORK_START,
    ID_WORK_END,
    ID_LUNCH_ENABLED,
    ID_LUNCH_START,
    ID_LUNCH_END,
    ID_THEME,
    ID_LANGUAGE,
    ID_MINIMAL_MODE,
    ID_FULLSCREEN_REMINDERS,
    ID_FOCUS_MODE,
    ID_FOCUS_SECONDS,
    ID_OVERLAY_STYLE,
    ID_OVERLAY_ACCENT,
    ID_WATER_GOAL_AUTO,
    ID_WATER_WEIGHT,
    ID_WATER_GENDER,
    ID_WATER_UNIT,
    ID_WATER_GOAL_OVERRIDE,
    ID_EYE_COUNTDOWN,
    ID_EYE_SILENT,
    ID_MOVEMENT_RANDOM,
    ID_MOVEMENT_SNOOZE,
    ID_SOUND,
    ID_AUTOSTART,
    ID_SAVE,
    ID_LOG_WATER,
    ID_STATUS,
    ID_DASHBOARD,
    ID_WATER_CHART,
    ID_LABEL_WATER_MINUTES,
    ID_LABEL_EYE_MINUTES,
    ID_LABEL_STAND_MINUTES,
    ID_LABEL_STAND_DURATION,
    ID_LABEL_DEFAULT_GLASS,
    ID_LABEL_EYE_COUNTDOWN,
    ID_LABEL_WEIGHT,
    ID_LABEL_GENDER,
    ID_LABEL_UNIT,
    ID_LABEL_GOAL_OVERRIDE,
    ID_LABEL_SNOOZE,
    ID_LABEL_WORK_HOURS,
    ID_LABEL_LUNCH_HOURS,
    ID_LABEL_THEME,
    ID_LABEL_LANGUAGE,
    ID_LABEL_FOCUS_SECONDS,
    ID_LABEL_OVERLAY_STYLE,
    ID_LABEL_OVERLAY_ACCENT,
    ID_SECTION_TODAY,
    ID_SECTION_REMINDERS,
    ID_SECTION_WATER,
    ID_SECTION_SCHEDULE,
    ID_SECTION_DISPLAY,
    ID_SECTION_APP,
    ID_LABEL_WORK_SEPARATOR,
    ID_LABEL_LUNCH_SEPARATOR,
    ID_NAV_REMINDERS,
    ID_NAV_WATER,
    ID_NAV_SCHEDULE,
    ID_NAV_DISPLAY,
    ID_NAV_APP,
];
