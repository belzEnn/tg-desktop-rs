// Client-specific geometry in logical pixels.
pub(crate) const GAP_TINY: u32 = 2;
pub(crate) const GAP_INLINE: u32 = 6;
pub(crate) const GAP: u32 = 8;
pub(crate) const GAP_SECTION: u32 = 12;
pub(crate) const SIGN_IN_PADDING: u16 = 24;

pub(crate) const WINDOW_SIZE: iced::Size = iced::Size::new(1024.0, 768.0);
pub(crate) const MIN_WINDOW_SIZE: iced::Size = iced::Size::new(680.0, 480.0);
pub(crate) const HEADER_HEIGHT: u32 = 51;
pub(crate) const PANE_DIVIDER: u32 = 1;
const SIDEBAR_FRACTION: f32 = 0.34;
const SIDEBAR_MIN: f32 = 224.0;
const SIDEBAR_MAX: f32 = 640.0;

pub(crate) fn sidebar_width(window_width: f32) -> f32 {
    (window_width * SIDEBAR_FRACTION)
        .round()
        .clamp(SIDEBAR_MIN, SIDEBAR_MAX)
}
pub(crate) const AVATAR_SIZE: u32 = 44;
pub(crate) const AVATAR_RADIUS: f32 = AVATAR_SIZE as f32 / 2.0;
pub(crate) const CHAT_ROW_PADDING: [u16; 2] = [7, 9];
pub(crate) const CHAT_ROW_RADIUS: f32 = 7.0;
pub(crate) const CHAT_ROW_HEIGHT: u32 = 60;
pub(crate) const SIDEBAR_INSET: u16 = 4;
pub(crate) const BADGE_RADIUS: f32 = 10.0;
pub(crate) const BADGE_PADDING: [u16; 2] = [2, 6];

pub(crate) const CONVERSATION_PADDING: u16 = 12;
pub(crate) const MESSAGE_PADDING: [u16; 2] = [8, 11];
pub(crate) const MESSAGE_GAP: u32 = 5;
pub(crate) const MESSAGE_GROUP_GAP: u32 = 2;
pub(crate) const MESSAGE_MAX_WIDTH: u32 = 430;
pub(crate) const BUBBLE_RADIUS: f32 = 12.0;
pub(crate) const EDITOR_RADIUS: f32 = 8.0;
pub(crate) const QUOTE_PADDING: [u16; 2] = [4, 8];
pub(crate) const COMPOSER_MIN_HEIGHT: u32 = 24;
pub(crate) const COMPOSER_PADDING: [u16; 2] = [5, 12];
pub(crate) const EDITOR_PADDING: [u16; 2] = [6, 8];
pub(crate) const ACTION_PADDING: [u16; 2] = [6, 10];
pub(crate) const COMPOSER_MAX_HEIGHT: u32 = 120;

pub(crate) const MENU_WIDTH: f32 = 152.0;
pub(crate) const MENU_ITEM_HEIGHT: f32 = 30.0;
pub(crate) const MENU_PADDING: f32 = 6.0;
pub(crate) const MENU_SPACING: f32 = 4.0;
pub(crate) const DIALOG_PADDING: u16 = 16;
