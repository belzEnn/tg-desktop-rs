// Existing project sizes, in logical pixels.
pub(crate) const GAP_TINY: u32 = 2;
pub(crate) const GAP_COMPACT: u32 = 3;
pub(crate) const GAP_SMALL: u32 = 4;
pub(crate) const GAP_INLINE: u32 = 6;
pub(crate) const GAP: u32 = 8;
pub(crate) const GAP_SECTION: u32 = 12;
pub(crate) const SIGN_IN_PADDING: u16 = 24;

// Client window geometry in logical pixels.
pub(crate) const WINDOW_SIZE: iced::Size = iced::Size::new(1024.0, 768.0);
pub(crate) const MIN_WINDOW_SIZE: iced::Size = iced::Size::new(680.0, 480.0);
pub(crate) const HEADER_HEIGHT: u32 = 56;
pub(crate) const PANE_DIVIDER: u32 = 1;
const SIDEBAR_FRACTION: f32 = 0.31;
const SIDEBAR_MIN: f32 = 224.0;
const SIDEBAR_MAX: f32 = 336.0;

pub(crate) fn sidebar_width(window_width: f32) -> f32 {
    (window_width * SIDEBAR_FRACTION)
        .round()
        .clamp(SIDEBAR_MIN, SIDEBAR_MAX)
}
pub(crate) const AVATAR_SIZE: u32 = 40;
pub(crate) const AVATAR_RADIUS: f32 = AVATAR_SIZE as f32 / 2.0;
pub(crate) const CHAT_ROW_PADDING: [u16; 2] = [8, 12];
pub(crate) const CHAT_ROW_RADIUS: f32 = 6.0;
pub(crate) const BADGE_PADDING: [u16; 2] = [2, 6];

pub(crate) const CONVERSATION_PADDING: u16 = 12;
pub(crate) const MESSAGE_PADDING: u16 = 10;
pub(crate) const MESSAGE_MAX_WIDTH: u32 = 480;
pub(crate) const QUOTE_PADDING: [u16; 2] = [4, 8];
pub(crate) const COMPOSER_MIN_HEIGHT: u32 = 42;
pub(crate) const COMPOSER_MAX_HEIGHT: u32 = 120;

pub(crate) const MENU_WIDTH: f32 = 152.0;
pub(crate) const MENU_ITEM_HEIGHT: f32 = 30.0;
pub(crate) const MENU_PADDING: f32 = 6.0;
pub(crate) const MENU_SPACING: f32 = 4.0;
pub(crate) const DIALOG_PADDING: u16 = 16;
