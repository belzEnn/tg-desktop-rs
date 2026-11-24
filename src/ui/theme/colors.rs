use iced::Color;

pub(crate) const AVATAR: Color = Color::from_rgb(0.25, 0.42, 0.62);
pub(crate) const LINK: Color = Color::from_rgb(0.26, 0.47, 0.75);
pub(crate) const CODE: Color = Color::from_rgba(0.48, 0.50, 0.54, 0.16);
pub(crate) const HIGHLIGHT: Color = Color::from_rgba(0.48, 0.50, 0.54, 0.24);
pub(crate) const HOVER_ALPHA: f32 = 0.10;

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
