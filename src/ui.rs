// Iced window and user interface.

mod components;
mod theme;

use crate::{app, domain::AuthorizationState, tg::Adapter};
use components::{composer, history, overlay, sidebar};
use iced::{
    Element, Fill,
    widget::{button, column, mouse_area, opaque, row, scrollable, space, stack, text, text_input},
};
use theme::metrics as m;

/// Runs the application window.
pub fn run(backend: Adapter) -> iced::Result {
    iced::application(move || app::boot(backend.clone()), app::update, view)
        .title(|_: &app::State| String::from("tg-desktop-rs"))
        .subscription(app::subscription)
        .run()
}

fn view(state: &app::State) -> Element<'_, app::Message> {
    let mut content = column![text("Sign in to Telegram")]
        .padding(m::SIGN_IN_PADDING)
        .spacing(m::GAP_SECTION);

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
            let open_chat = state
                .open_chat_id()
                .and_then(|id| state.chats().iter().find(|chat| chat.id == id));
            let mut conversation = column![history::view(state, open_chat)];
            if open_chat.is_some() {
                conversation = conversation.push(composer::view(state));
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
            .padding(m::CONVERSATION_PADDING)
            .width(iced::Length::Fill)
            .height(iced::Length::Fill);
            let mut layers = stack![
                row![sidebar::sidebar(state), chat_area]
                    .width(Fill)
                    .height(Fill)
            ]
            .width(Fill)
            .height(Fill);
            if let Some(confirmation) = state.delete_confirmation() {
                layers = layers
                    .push(opaque(
                        mouse_area(space().width(Fill).height(Fill))
                            .on_press(app::Message::CancelDelete),
                    ))
                    .push(overlay::delete_confirmation(
                        confirmation,
                        state
                            .messages()
                            .iter()
                            .find(|message| message.id == confirmation.message_id),
                        state.mutation_pending(),
                        state.message_action_error(),
                    ));
            } else if let Some(menu) = state.message_menu() {
                layers = layers
                    .push(
                        mouse_area(space().width(Fill).height(Fill))
                            .on_press(app::Message::DismissMessageMenu),
                    )
                    .push(overlay::message_menu(
                        menu,
                        !state.is_editing(),
                        !state.forward_pending(),
                        !state.is_editing() && !state.mutation_pending(),
                    ));
            }
            return layers.into();
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
