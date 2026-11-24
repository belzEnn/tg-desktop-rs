use iced::{
    Border, Color, Theme,
    widget::{button, container, rule, text, text_editor},
};

use super::{colors, metrics};

pub(in crate::ui) fn chat_button(
    theme: &Theme,
    status: button::Status,
    selected: bool,
) -> button::Style {
    let background = match (selected, status) {
        (true, button::Status::Hovered | button::Status::Pressed) => Some(
            theme
                .palette()
                .primary
                .scale_alpha(colors::SELECTED_HOVER_ALPHA),
        ),
        (true, _) => Some(theme.palette().primary.scale_alpha(colors::SELECTED_ALPHA)),
        (false, button::Status::Hovered | button::Status::Pressed) => {
            Some(theme.palette().text.scale_alpha(colors::HOVER_ALPHA))
        }
        (false, _) => None,
    };
    button::Style {
        background: background.map(Into::into),
        text_color: if status == button::Status::Disabled && !selected {
            theme.palette().text.scale_alpha(colors::DISABLED_ALPHA)
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

fn shade(theme: &Theme, light: Color, dark: Color) -> Color {
    if theme.extended_palette().is_dark {
        dark
    } else {
        light
    }
}

pub(in crate::ui) fn muted_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(shade(theme, colors::LIGHT_MUTED, colors::DARK_MUTED)),
    }
}

fn border_color(theme: &Theme) -> Color {
    shade(theme, colors::LIGHT_BORDER, colors::DARK_BORDER)
}

pub(in crate::ui) fn divider(theme: &Theme) -> rule::Style {
    rule::Style {
        color: border_color(theme),
        ..rule::default(theme)
    }
}

pub(in crate::ui) fn surface(theme: &Theme) -> container::Style {
    container::Style::default().background(shade(
        theme,
        colors::LIGHT_SURFACE,
        colors::DARK_SURFACE,
    ))
}

pub(in crate::ui) fn conversation(theme: &Theme) -> container::Style {
    container::Style::default().background(shade(
        theme,
        colors::LIGHT_CONVERSATION,
        colors::DARK_CONVERSATION,
    ))
}

pub(in crate::ui) fn avatar(_: &Theme) -> container::Style {
    container::Style::default()
        .background(colors::AVATAR)
        .color(Color::WHITE)
        .border(Border::default().rounded(metrics::AVATAR_RADIUS))
}

pub(in crate::ui) fn badge(theme: &Theme) -> container::Style {
    container::primary(theme).border(Border::default().rounded(metrics::BADGE_RADIUS))
}

pub(in crate::ui) fn bubble(theme: &Theme, outgoing: bool) -> container::Style {
    let style = if outgoing {
        container::Style::default().background(shade(
            theme,
            colors::LIGHT_OUTGOING,
            colors::DARK_OUTGOING,
        ))
    } else {
        surface(theme)
    };
    style.border(Border::default().rounded(metrics::BUBBLE_RADIUS))
}

pub(in crate::ui) fn quote(theme: &Theme) -> container::Style {
    container::Style::default()
        .background(theme.palette().text.scale_alpha(colors::HOVER_ALPHA))
        .border(Border::default().rounded(metrics::CHAT_ROW_RADIUS))
}

pub(in crate::ui) fn composer(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let mut style = text_editor::default(theme, status);
    style.border.radius = metrics::EDITOR_RADIUS.into();
    style.placeholder = shade(theme, colors::LIGHT_MUTED, colors::DARK_MUTED);
    if matches!(status, text_editor::Status::Active) {
        style.border.color = border_color(theme);
    }
    style
}
