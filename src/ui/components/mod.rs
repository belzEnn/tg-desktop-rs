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

use super::theme::{metrics as m, typography as ty};

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
            .style(button::secondary)
            .on_press_maybe((!logout_pending).then_some(app::Message::LogOut)),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(m::GAP),
    )
    .width(Fill)
    .height(m::HEADER_HEIGHT)
    .align_y(iced::Alignment::Center)
    .padding([0, m::CONVERSATION_PADDING])
    .into()
}
