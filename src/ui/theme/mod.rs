//! Shared UI values. Sizes are logical pixels.

pub(super) mod appearance;
pub(super) mod colors;
pub(super) mod metrics;
pub(super) mod typography;

use std::sync::LazyLock;

use iced::Theme;

use crate::config::ColorScheme;

pub(super) fn iced_theme(scheme: ColorScheme) -> Theme {
    static LIGHT: LazyLock<Theme> = LazyLock::new(|| Theme::custom("Light", colors::LIGHT));
    static DARK: LazyLock<Theme> = LazyLock::new(|| Theme::custom("Dark", colors::DARK));

    match scheme {
        ColorScheme::Light => LIGHT.clone(),
        ColorScheme::Dark => DARK.clone(),
    }
}
