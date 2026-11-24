use iced::{
    Border, Theme,
    widget::{button, text},
};

use super::{colors, metrics};

pub(in crate::ui) fn chat_button(
    theme: &Theme,
    status: button::Status,
    selected: bool,
) -> button::Style {
    let palette = theme.extended_palette();
    let background = match (selected, status) {
        (true, button::Status::Hovered | button::Status::Pressed) => {
            Some(palette.primary.weak.color.scale_alpha(0.48))
        }
        (true, _) => Some(palette.primary.weak.color.scale_alpha(0.32)),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            Some(theme.palette().text.scale_alpha(colors::HOVER_ALPHA))
        }
        (false, _) => None,
    };
    button::Style {
        background: background.map(Into::into),
        text_color: if status == button::Status::Disabled && !selected {
            theme.palette().text.scale_alpha(0.5)
        } else {
            theme.palette().text
        },
        border: Border::default().rounded(metrics::CHAT_ROW_RADIUS),
        ..button::Style::default()
    }
}

pub(in crate::ui) fn error_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().danger),
    }
}
