use crate::ui::theme::{appearance, colors, metrics as m, typography as ty};
use crate::{app, domain::ChatSummary};
use iced::{
    Element, Fill, Length,
    widget::{button, column, container, row, scrollable, text},
};

pub(in crate::ui) fn view(state: &app::State, width: f32) -> Element<'_, app::Message> {
    let forwarding = state.forward_selection().is_some();
    let mut chats = column![].spacing(m::GAP_TINY);
    for chat in state.chats() {
        chats = chats.push(chat_row(state, chat, forwarding));
    }
    container(scrollable(chats).height(Length::Fill))
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
    let mut heading =
        row![text(&chat.title).width(Fill).wrapping(text::Wrapping::None)].spacing(m::GAP_INLINE);
    if chat.unread_count > 0 {
        heading = heading.push(
            container(text(chat.unread_count.to_string()).size(ty::CAPTION))
                .padding(m::BADGE_PADDING)
                .style(container::primary),
        );
    }
    button(
        container(
            row![
                container(text(initial))
                    .width(m::AVATAR_SIZE)
                    .height(m::AVATAR_SIZE)
                    .center(m::AVATAR_SIZE)
                    .style(|_: &iced::Theme| iced::widget::container::Style {
                        background: Some(iced::Background::Color(colors::AVATAR)),
                        text_color: Some(iced::Color::WHITE),
                        border: iced::Border::default().rounded(m::AVATAR_RADIUS),
                        ..Default::default()
                    }),
                column![
                    heading,
                    text(preview)
                        .size(ty::PREVIEW)
                        .width(iced::Length::Fill)
                        .wrapping(iced::widget::text::Wrapping::None),
                ]
                .spacing(m::GAP_SMALL)
                .width(iced::Length::Fill),
            ]
            .spacing(m::GAP_SECTION)
            .align_y(iced::Alignment::Center),
        )
        .width(iced::Length::Fill)
        .padding(m::CHAT_ROW_PADDING),
    )
    .width(iced::Length::Fill)
    .clip(true)
    .style(move |theme, status| appearance::chat_button(theme, status, selected))
    .on_press_maybe(if forwarding {
        (chat.can_receive_forwards
            && !state.forward_pending()
            && state.open_chat_id() != Some(chat.id))
        .then_some(app::Message::SelectForwardDestination(chat.id))
    } else {
        Some(app::Message::OpenChat(chat.id))
    })
    .into()
}

fn single_line_preview(message: &str) -> String {
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

    preview
}
