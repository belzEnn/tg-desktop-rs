pub(super) mod composer;
pub(super) mod history;
pub(super) mod message;
pub(super) mod overlay;
pub(super) mod sidebar;

use iced::{
    Element, Fill,
    widget::{button, container, row, text},
};

use crate::{app, domain::ChatSummary};

use super::theme::{appearance, metrics as m, typography as ty};

pub(super) fn chat_header<'a>(
    state: &app::State,
    chat: Option<&'a ChatSummary>,
) -> Element<'a, app::Message> {
    let title = chat.map_or("", |chat| chat.title.as_str());
    let logout_pending = state.logout_pending();
    container(
        row![
            container(
                text(title)
                    .size(ty::CHAT_TITLE)
                    .font(ty::HEADING_FONT)
                    .wrapping(text::Wrapping::None)
            )
            .width(Fill)
            .clip(true),
            super::theme_button(state.color_scheme()),
            button(if logout_pending {
                "Logging out…"
            } else {
                "Log out"
            })
            .style(appearance::toolbar_action)
            .padding(m::ACTION_PADDING)
            .on_press_maybe((!logout_pending).then_some(app::Message::LogOut)),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(m::GAP),
    )
    .width(Fill)
    .height(m::HEADER_HEIGHT)
    .style(appearance::surface)
    .align_y(iced::Alignment::Center)
    .padding([0, m::CONVERSATION_PADDING])
    .into()
}

fn time_label(unix_seconds: Option<i32>, prefix: &str, suffix: &str) -> String {
    if let Some(seconds) = unix_seconds {
        // TDLib supplies Unix seconds. Both surfaces use UTC.
        let minutes = seconds / 60;
        format!(
            "{prefix}{:02}:{:02}{suffix}",
            minutes / 60 % 24,
            minutes % 60
        )
    } else {
        format!("{} {}", prefix.trim_end(), suffix.trim_start())
            .trim()
            .to_owned()
    }
}

fn delivery_mark(message_id: i64, read_through: Option<i64>) -> Option<&'static str> {
    let read_through = read_through?;
    (message_id > 0).then_some(if message_id <= read_through {
        "✓✓"
    } else {
        "✓"
    })
}
