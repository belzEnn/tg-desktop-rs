use std::borrow::Cow;

use crate::ui::theme::{appearance, metrics as m, typography as ty};
use crate::{app, domain::ChatSummary};
use iced::{
    Element, Fill, Length,
    widget::{button, column, container, row, scrollable, text},
};

pub(in crate::ui) fn view(state: &app::State, width: f32) -> Element<'_, app::Message> {
    let forwarding = state.forward_selection().is_some();
    let mut chats = column![].spacing(m::GAP_TINY).padding(m::SIDEBAR_INSET);
    for chat in state.chats() {
        chats = chats.push(chat_row(state, chat, forwarding));
    }
    container(scrollable(chats).height(Length::Fill))
        .style(appearance::surface)
        .width(width)
        .height(Length::Fill)
        .into()
}

fn chat_row<'a>(
    state: &app::State,
    chat: &'a ChatSummary,
    forwarding: bool,
) -> Element<'a, app::Message> {
    let selected = state.open_chat_id() == Some(chat.id);
    let preview = single_line_preview(&chat.last_message_preview);
    let initial = chat
        .title
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let destination = if forwarding {
        (chat.can_receive_forwards && !state.forward_pending() && !selected)
            .then_some(app::Message::SelectForwardDestination(chat.id))
    } else {
        Some(app::Message::OpenChat(chat.id))
    };
    let mut heading = row![
        container(
            text(&chat.title)
                .size(ty::ROW_TITLE)
                .font(ty::HEADING_FONT)
                .wrapping(text::Wrapping::None)
        )
        .width(Fill)
        .clip(true),
    ]
    .align_y(iced::Alignment::Center)
    .spacing(m::GAP_INLINE);
    if chat.unread_count > 0 {
        heading = heading.push(
            container(text(chat.unread_count.to_string()).size(ty::CAPTION))
                .padding(m::BADGE_PADDING)
                .style(appearance::badge),
        );
    }
    let mut preview = text(preview)
        .size(ty::PREVIEW)
        .width(Fill)
        .wrapping(text::Wrapping::None);
    if destination.is_some() || selected {
        preview = preview.style(appearance::muted_text);
    }
    button(
        row![
            container(text(initial).font(ty::HEADING_FONT))
                .center(m::AVATAR_SIZE)
                .style(appearance::avatar),
            column![heading, container(preview).width(Fill).clip(true)]
                .spacing(m::GAP_TINY)
                .width(Fill),
        ]
        .spacing(m::GAP_SECTION)
        .align_y(iced::Alignment::Center),
    )
    .padding(m::CHAT_ROW_PADDING)
    .height(m::CHAT_ROW_HEIGHT)
    .width(iced::Length::Fill)
    .clip(true)
    .style(move |theme, status| appearance::chat_button(theme, status, selected))
    .on_press_maybe(destination)
    .into()
}

fn single_line_preview(message: &str) -> Cow<'_, str> {
    let mut previous_was_whitespace = true;
    if !message.chars().any(|character| {
        let whitespace = character.is_whitespace();
        let needs_normalizing = whitespace && (previous_was_whitespace || character != ' ');
        previous_was_whitespace = whitespace;
        needs_normalizing
    }) {
        return Cow::Borrowed(message);
    }

    let mut preview = String::with_capacity(message.len());
    let mut previous_was_whitespace = true;

    for character in message.chars() {
        if character.is_whitespace() {
            if !previous_was_whitespace {
                preview.push(' ');
            }
            previous_was_whitespace = true;
        } else {
            preview.push(character);
            previous_was_whitespace = false;
        }
    }

    Cow::Owned(preview)
}
