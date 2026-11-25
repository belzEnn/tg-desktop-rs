use super::message::reply_preview;
use crate::ui::theme::{appearance, metrics as m, typography as ty};
use crate::{app, domain::MessagePreview};
use iced::{
    Element, Fill,
    keyboard::{Key, key::Named},
    widget::{button, column, container, row, text, text_editor},
};

pub(in crate::ui) fn view(state: &app::State) -> Element<'_, app::Message> {
    let forward_selection = state.forward_selection();
    let editing = state.is_editing();
    let editing_pending = editing && state.mutation_pending();
    let composer = text_editor(state.composer())
        .id(app::CHAT_COMPOSER_ID)
        .size(ty::BODY_SIZE)
        .line_height(ty::MESSAGE_LINE_HEIGHT)
        .style(appearance::composer)
        .placeholder("Write a message")
        .padding(m::EDITOR_PADDING)
        .min_height(m::COMPOSER_MIN_HEIGHT)
        .max_height(m::COMPOSER_MAX_HEIGHT)
        .on_action(app::Message::ComposerAction)
        .key_binding(|press| {
            if press.modified_key == Key::Named(Named::Enter) {
                if press.modifiers.shift() {
                    Some(text_editor::Binding::Enter)
                } else {
                    Some(text_editor::Binding::Custom(app::Message::Submit))
                }
            } else {
                text_editor::Binding::from_key_press(press)
            }
        });
    let mut conversation = column![];
    if let Some(error) = state.send_error() {
        conversation = conversation.push(text(error).style(appearance::error_text));
    }
    if let Some(selection) = forward_selection {
        if let Some(source) = state.forward_message() {
            conversation = conversation.push(
                text(format!(
                    "Forward message: {}",
                    MessagePreview::message_snippet(&source.text, &source.text_runs)
                ))
                .size(ty::CAPTION),
            );
        }
        let destination = selection.destination.and_then(|id| state.chat(id));
        let caption = if state.forward_pending() {
            "Forwarding…".to_owned()
        } else if let Some(chat) = destination {
            format!("Forward to {}", chat.title)
        } else {
            "Choose a destination in the chat list".to_owned()
        };
        conversation = conversation.push(
            row![
                text(caption).width(iced::Length::Fill),
                button("Forward").on_press_maybe(
                    destination
                        .filter(|chat| state.can_forward_to(chat.id))
                        .map(|_| app::Message::ConfirmForward)
                ),
                button("Cancel").on_press_maybe(
                    (!state.forward_pending()).then_some(app::Message::CancelForward)
                ),
            ]
            .align_y(iced::Alignment::Center)
            .spacing(m::GAP),
        );
    }
    if state.forward_pending() && forward_selection.is_none() {
        conversation = conversation.push(text("Forwarding…"));
    }
    if let Some(status) = state.forward_status() {
        conversation = conversation.push(text(status));
    }
    if state.delete_confirmation().is_none()
        && let Some(error) = state.message_action_error()
    {
        conversation = conversation.push(text(error).style(appearance::error_text));
    }
    if editing {
        conversation = conversation.push(
            row![
                text(if editing_pending {
                    "Saving changes…"
                } else {
                    "Editing message"
                })
                .width(Fill),
                button("Cancel")
                    .on_press_maybe((!editing_pending).then_some(app::Message::CancelEdit)),
            ]
            .align_y(iced::Alignment::Center)
            .spacing(m::GAP),
        );
    } else if let Some(original) = state.reply_message() {
        let snippet = MessagePreview::message_snippet(&original.text, &original.text_runs);
        conversation = conversation.push(
            row![
                reply_preview(Some(&original.sender_name), snippet.into_string(), Fill),
                button("Cancel reply").on_press(app::Message::CancelReply),
            ]
            .align_y(iced::Alignment::Center)
            .spacing(m::GAP),
        );
    }
    conversation = conversation.push(
        row![
            composer,
            button(if editing { "Save" } else { "Send" })
                .style(appearance::toolbar_action)
                .padding(m::ACTION_PADDING)
                .on_press_maybe((!editing_pending).then_some(app::Message::Submit)),
        ]
        .spacing(m::GAP)
        .align_y(iced::Alignment::Center),
    );
    container(conversation.spacing(m::GAP))
        .style(appearance::surface)
        .padding(m::COMPOSER_PADDING)
        .width(Fill)
        .into()
}
