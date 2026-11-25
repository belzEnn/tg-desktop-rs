use std::borrow::Cow;

use super::{delivery_mark, time_label};

use crate::ui::theme::{appearance, metrics as m, typography as ty};
use crate::{app, domain::ChatSummary};
use iced::{
    Element, Fill, Length,
    widget::{button, column, container, row, scrollable, text},
};

pub(in crate::ui) fn view(state: &app::State, width: f32) -> Element<'_, app::Message> {
    let forwarding = state.forward_selection().is_some();
    let mut chats = column![].padding(m::SIDEBAR_INSET);
    for chat in state.chats() {
        chats = chats.push(chat_row(state, chat, forwarding));
    }
    if let Some(error) = state.chat_page_error() {
        chats = chats
            .push(text(error).style(appearance::error_text))
            .push(button("Retry").on_press(app::Message::RetryChatPage));
    } else if state.chat_page_loading() {
        chats = chats.push(text("Loading chats…").style(appearance::muted_text));
    }
    let list = scrollable(chats)
        .height(Length::Fill)
        .on_scroll(|viewport| {
            let offset_y = viewport.absolute_offset().y;
            let viewport_height = viewport.bounds().height;
            let remaining_y = viewport.content_bounds().height - viewport_height - offset_y;
            app::Message::ChatsScrolled {
                offset_y,
                remaining_y,
                viewport_height,
            }
        });
    container(list)
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
        .find(|character| !character.is_whitespace())
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
    let mark = chat
        .last_message_id
        .filter(|_| chat.last_message_is_outgoing)
        .and_then(|id| delivery_mark(id, chat.last_read_outbox_message_id));
    let enabled = destination.is_some() || selected;
    if chat.last_message_date.is_some() || mark.is_some() {
        let prefix = match mark {
            Some("✓✓") => "✓✓ ",
            Some(_) => "✓ ",
            None => "",
        };
        let mut time =
            text(time_label(chat.last_message_date, prefix, "")).size(ty::DIALOG_METADATA);
        if enabled {
            time = time.style(appearance::muted_text);
        }
        heading = heading.push(time);
    }
    let mut preview = text(preview)
        .size(ty::PREVIEW)
        .width(Fill)
        .wrapping(text::Wrapping::None);
    if enabled {
        preview = preview.style(appearance::muted_text);
    }
    let preview = container(preview).width(Fill).clip(true);
    let details: Element<'_, _> = if chat.unread_count == 0 {
        preview.into()
    } else {
        row![
            preview,
            container(text(chat.unread_count.to_string()).size(ty::CAPTION))
                .padding(m::BADGE_PADDING)
                .style(appearance::badge),
        ]
        .align_y(iced::Alignment::Center)
        .spacing(m::GAP_INLINE)
        .into()
    };
    button(
        row![
            container(text(initial).font(ty::HEADING_FONT))
                .center(m::AVATAR_SIZE)
                .style(appearance::avatar),
            column![heading, details].spacing(m::GAP_TINY).width(Fill),
        ]
        .spacing(m::GAP)
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
