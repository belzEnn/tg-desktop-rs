// Iced window and user interface.

use iced::{
    Element,
    keyboard::{Key, key::Named},
    widget::{button, column, container, row, scrollable, text, text_editor, text_input},
};

use crate::{app, domain::AuthorizationState, tg::Adapter};

/// Runs the application window.
pub fn run(backend: Adapter) -> iced::Result {
    iced::application(move || app::boot(backend.clone()), app::update, view)
        .title(|_: &app::State| String::from("tg-desktop-rs"))
        .subscription(app::subscription)
        .run()
}

fn view(state: &app::State) -> Element<'_, app::Message> {
    let mut content = column![text("Sign in to Telegram")].padding(24).spacing(12);

    match state.authorization_state() {
        Some(AuthorizationState::WaitPhoneNumber) => {
            content = content
                .push(text("Enter your phone number with country code"))
                .push(
                    text_input("+123456789", state.phone())
                        .on_input(app::Message::PhoneChanged)
                        .on_submit(app::Message::SubmitPhone),
                )
                .push(button("Continue").on_press(app::Message::SubmitPhone));
        }
        Some(AuthorizationState::WaitCode) => {
            content = content
                .push(text("Enter the login code sent by Telegram"))
                .push(
                    text_input("Login code", state.code())
                        .on_input(app::Message::CodeChanged)
                        .on_submit(app::Message::SubmitCode),
                )
                .push(button("Continue").on_press(app::Message::SubmitCode));
        }
        Some(AuthorizationState::WaitPassword) => {
            content = content
                .push(text("Enter your two-step verification password"))
                .push(
                    text_input("Password", state.password())
                        .on_input(app::Message::PasswordChanged)
                        .secure(true)
                        .on_submit(app::Message::SubmitPassword),
                )
                .push(button("Continue").on_press(app::Message::SubmitPassword));
        }
        Some(AuthorizationState::WaitRegistration) => {
            content = content.push(text("Account registration is not supported yet."));
        }
        Some(AuthorizationState::WaitTdlibParameters) => {
            content = content.push(text(
                "Initializing Telegram. The login form will appear when ready.",
            ));
        }
        Some(AuthorizationState::Ready) => {
            let mut chats = column![].spacing(2);
            for chat in state.chats() {
                let preview = single_line_preview(&chat.last_message_preview);
                let initial = chat
                    .title
                    .chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();
                chats = chats.push(
                    button(
                        container(
                            row![
                                container(text(initial))
                                    .width(40)
                                    .height(40)
                                    .center(40)
                                    .style(|_: &iced::Theme| iced::widget::container::Style {
                                        background: Some(iced::Background::Color(
                                            iced::Color::from_rgb(0.25, 0.42, 0.62,)
                                        )),
                                        border: iced::Border::default().rounded(20.0),
                                        ..Default::default()
                                    }),
                                column![
                                    text(&chat.title),
                                    text(preview)
                                        .size(13)
                                        .width(iced::Length::Fill)
                                        .wrapping(iced::widget::text::Wrapping::None),
                                ]
                                .spacing(4)
                                .width(iced::Length::Fill),
                            ]
                            .spacing(12)
                            .align_y(iced::Alignment::Center),
                        )
                        .width(iced::Length::Fill)
                        .padding([8, 12]),
                    )
                    .width(iced::Length::Fill)
                    .style(|theme: &iced::Theme, status| iced::widget::button::Style {
                        background: match status {
                            iced::widget::button::Status::Hovered
                            | iced::widget::button::Status::Pressed => {
                                Some(iced::Background::Color(iced::Color::from_rgba(
                                    theme.palette().text.r,
                                    theme.palette().text.g,
                                    theme.palette().text.b,
                                    0.10,
                                )))
                            }
                            _ => None,
                        },
                        text_color: theme.palette().text,
                        border: iced::Border::default().rounded(6.0),
                        ..Default::default()
                    })
                    .on_press(app::Message::OpenChat(chat.id)),
                );
            }
            let sidebar = container(scrollable(chats).height(iced::Length::Fill))
                .width(360)
                .height(iced::Length::Fill);
            let open_chat = state
                .open_chat_id()
                .and_then(|id| state.chats().iter().find(|chat| chat.id == id));
            let mut history = column![].spacing(8);
            if let Some(chat) = open_chat {
                history = history.push(text(&chat.title).size(20));
                if state.history_loading() {
                    history = history.push(text("Loading messages…"));
                } else if state.messages().is_empty() && state.history_error().is_none() {
                    history = history.push(text("No messages in this chat."));
                }
                if let Some(error) = state.history_error() {
                    history = history.push(text(error));
                }
                for message in state.messages() {
                    history = history.push(
                        container(
                            column![
                                text(&message.sender_name).size(12),
                                text(&message.text).wrapping(iced::widget::text::Wrapping::Word),
                            ]
                            .spacing(3),
                        )
                        .padding(10),
                    );
                }
            } else {
                history = history.push(text("Select a chat to view its messages."));
            }
            let composer = text_editor(state.composer())
                .placeholder("Write a message")
                .min_height(42)
                .max_height(120)
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
            let mut conversation = column![scrollable(history).height(iced::Length::Fill)];
            if open_chat.is_some() {
                if let Some(error) = state.send_error() {
                    conversation = conversation.push(text(error));
                }
                conversation = conversation.push(
                    row![composer, button("Send").on_press(app::Message::Submit),]
                        .spacing(8)
                        .align_y(iced::Alignment::End),
                );
            }
            let chat_area = column![
                row![
                    text("").width(iced::Length::Fill),
                    button(if state.logout_pending() {
                        "Logging out…"
                    } else {
                        "Log out"
                    })
                    .on_press(app::Message::LogOut),
                ]
                .align_y(iced::Alignment::Center),
                conversation,
            ]
            .padding(12)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill);
            return row![sidebar, chat_area].height(iced::Length::Fill).into();
        }
        Some(AuthorizationState::Closed) => {
            content = content.push(text("Telegram connection closed"));
        }
        Some(state) => {
            content = content.push(text(format!("Connecting: {}", state.as_str())));
        }
        None => {
            content = content.push(text("Connecting to Telegram…"));
        }
    }

    if let Some(error) = state.authorization_error() {
        content = content.push(text(error));
    }

    scrollable(content).into()
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
