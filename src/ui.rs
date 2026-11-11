// Iced window and user interface.

use iced::Element;

/// Runs the empty application window.
pub fn run() -> iced::Result {
    iced::application(|| (), update, view)
        .title(|_: &()| String::from("tg-desktop-rs"))
        .run()
}

fn update(_: &mut (), _: ()) {}

fn view(_: &()) -> Element<'_, ()> {
    iced::widget::column![].into()
}
