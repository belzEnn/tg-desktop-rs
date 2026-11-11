// Iced window and user interface.

use iced::{
    Element,
    widget::{button, column, container, row, scrollable, text, text_input},
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
                    container(
                        row![
                            container(text(initial))
                                .width(40)
                                .height(40)
                                .center(40)
                                .style(|_| iced::widget::container::Style {
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
                );
            }
            let sidebar = container(scrollable(chats).height(iced::Length::Fill))
                .width(360)
                .height(iced::Length::Fill);
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
                container(text("")).height(iced::Length::Fill),
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
