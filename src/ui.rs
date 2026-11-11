// Iced window and user interface.

use iced::{
    Element,
    widget::{column, text},
};

use crate::{app, tg::Adapter};

/// Runs the application window.
pub fn run(backend: Adapter) -> iced::Result {
    iced::application(move || app::boot(backend.clone()), app::update, view)
        .title(|_: &app::State| String::from("tg-desktop-rs"))
        .subscription(app::subscription)
        .run()
}

fn view(state: &app::State) -> Element<'_, app::Message> {
    column![
        text("Telegram connection status"),
        text(state.authorization_status()),
    ]
    .padding(24)
    .spacing(12)
    .into()
}
