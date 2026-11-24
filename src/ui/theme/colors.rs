use iced::Color;

pub(crate) const AVATAR: Color = Color::from_rgb(0.25, 0.42, 0.62);
pub(crate) const CODE: Color = Color::from_rgba(0.48, 0.50, 0.54, 0.16);
pub(crate) const HIGHLIGHT: Color = Color::from_rgba(0.48, 0.50, 0.54, 0.24);
pub(super) const HOVER_ALPHA: f32 = 0.08;
pub(super) const DISABLED_ALPHA: f32 = 0.5;
pub(super) const SELECTED_ALPHA: f32 = 0.19;
pub(super) const SELECTED_HOVER_ALPHA: f32 = 0.27;

pub(super) const LIGHT_SURFACE: Color = Color::from_rgb8(255, 255, 255);
pub(super) const DARK_SURFACE: Color = Color::from_rgb8(31, 40, 54);
pub(super) const LIGHT_CONVERSATION: Color = Color::from_rgb8(232, 239, 245);
pub(super) const DARK_CONVERSATION: Color = Color::from_rgb8(20, 27, 38);
pub(super) const LIGHT_OUTGOING: Color = Color::from_rgb8(220, 241, 234);
pub(super) const DARK_OUTGOING: Color = Color::from_rgb8(27, 62, 68);
pub(super) const LIGHT_MUTED: Color = Color::from_rgb8(78, 99, 116);
pub(super) const DARK_MUTED: Color = Color::from_rgb8(162, 181, 202);
pub(super) const LIGHT_BORDER: Color = Color::from_rgb8(199, 212, 223);
pub(super) const DARK_BORDER: Color = Color::from_rgb8(62, 80, 100);

// Application palette. These colors were chosen for this client.
pub(super) const LIGHT: iced::theme::Palette = iced::theme::Palette {
    background: Color::from_rgb8(246, 248, 251),
    text: Color::from_rgb8(24, 35, 48),
    primary: Color::from_rgb8(34, 104, 165),
    success: Color::from_rgb8(30, 115, 85),
    warning: Color::from_rgb8(138, 85, 24),
    danger: Color::from_rgb8(175, 49, 59),
};

pub(super) const DARK: iced::theme::Palette = iced::theme::Palette {
    background: Color::from_rgb8(23, 30, 42),
    text: Color::from_rgb8(229, 236, 246),
    primary: Color::from_rgb8(127, 190, 246),
    success: Color::from_rgb8(117, 204, 160),
    warning: Color::from_rgb8(241, 189, 109),
    danger: Color::from_rgb8(255, 157, 164),
};

pub(in crate::ui) fn link(scheme: crate::config::ColorScheme) -> Color {
    match scheme {
        crate::config::ColorScheme::Light => LIGHT.primary,
        crate::config::ColorScheme::Dark => DARK.primary,
    }
}
