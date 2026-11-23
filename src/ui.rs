// Iced window and user interface.

use iced::{
    Element, Event, Fill, Length, Point, Rectangle, Renderer, Size, Theme,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, renderer,
        widget::{Operation, Tree, tree},
    },
    keyboard::{Key, key::Named},
    mouse,
    widget::{
        button, column, container, mouse_area, opaque, pin, responsive, row, scrollable, space,
        stack, text, text_editor, text_input,
    },
};

use crate::{
    app,
    domain::{AuthorizationState, MessagePreview},
    tg::Adapter,
};

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
            let forward_selection = state.forward_selection();
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
                    .on_press_maybe(if forward_selection.is_some() {
                        (chat.can_receive_forwards
                            && !state.forward_pending()
                            && state.open_chat_id() != Some(chat.id))
                        .then_some(app::Message::SelectForwardDestination(chat.id))
                    } else {
                        Some(app::Message::OpenChat(chat.id))
                    }),
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
                    let mut body = column![text(&message.sender_name).size(12)].spacing(3);
                    if let Some(origin) = &message.forwarded_from {
                        body = body.push(text(format!("Forwarded from {origin}")).size(12));
                    }
                    if let Some(reply) = &message.reply {
                        let (sender, snippet) = match &reply.preview {
                            Some(preview) => {
                                (preview.sender_name.as_deref(), preview.text.as_ref())
                            }
                            None => (
                                None,
                                reply
                                    .quote
                                    .as_deref()
                                    .unwrap_or("Loading original message…"),
                            ),
                        };
                        body = body.push(reply_preview(sender, snippet));
                    }
                    body =
                        body.push(text(&message.text).wrapping(iced::widget::text::Wrapping::Word));
                    let content = container(body).padding(10);
                    history = if message.can_reference {
                        history.push(mouse_area(content).on_right_press(HistoryEvent::OpenMenu {
                            chat_id: chat.id,
                            message_id: message.id,
                        }))
                    } else {
                        history.push(content)
                    };
                }
            } else {
                history = history.push(text("Select a chat to view its messages."));
            }
            let composer = text_editor(state.composer())
                .id(app::CHAT_COMPOSER_ID)
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
            let history = scrollable(history)
                .id(app::CHAT_HISTORY_SCROLL_ID)
                .on_scroll(|viewport| HistoryEvent::Scrolled {
                    offset_y: viewport.absolute_offset().y,
                    content_height: viewport.content_bounds().height,
                })
                .height(iced::Length::Fill);
            let mut conversation = column![Element::new(History(history.into()))];
            if open_chat.is_some() {
                if let Some(error) = state.send_error() {
                    conversation = conversation.push(text(error));
                }
                if let Some(selection) = forward_selection {
                    if let Some(source) = state.forward_message() {
                        conversation = conversation.push(
                            text(format!(
                                "Forward message: {}",
                                MessagePreview::snippet(&source.text)
                            ))
                            .size(12),
                        );
                    }
                    let destination = selection
                        .destination
                        .and_then(|id| state.chats().iter().find(|chat| chat.id == id));
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
                        .spacing(8),
                    );
                }
                if state.forward_pending() && forward_selection.is_none() {
                    conversation = conversation.push(text("Forwarding…"));
                }
                if let Some(status) = state.forward_status() {
                    conversation = conversation.push(text(status));
                }
                if let Some(original) = state.reply_message() {
                    let snippet = MessagePreview::snippet(&original.text);
                    conversation = conversation.push(
                        row![
                            reply_preview(Some(&original.sender_name), snippet.into_string()),
                            button("Cancel reply").on_press(app::Message::CancelReply),
                        ]
                        .align_y(iced::Alignment::Center)
                        .spacing(8),
                    );
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
            let mut layers = stack![row![sidebar, chat_area].width(Fill).height(Fill)]
                .width(Fill)
                .height(Fill);
            if let Some(menu) = state.message_menu() {
                layers = layers
                    .push(
                        mouse_area(space().width(Fill).height(Fill))
                            .on_press(app::Message::DismissMessageMenu),
                    )
                    .push(message_menu(menu, !state.forward_pending()));
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

const MENU_WIDTH: f32 = 140.0;
const MENU_HEIGHT: f32 = 76.0;

fn message_menu(menu: app::MessageMenu, can_forward: bool) -> Element<'static, app::Message> {
    responsive(move |available| {
        let actions = column![
            button("Reply")
                .width(Fill)
                .height(30)
                .on_press(app::Message::SelectReply(menu.message_id)),
            button("Forward")
                .width(Fill)
                .height(30)
                .on_press_maybe(can_forward.then_some(app::Message::StartForward(menu.message_id))),
        ]
        .spacing(4);
        let panel = container(actions)
            .width(MENU_WIDTH)
            .height(MENU_HEIGHT)
            .padding(6)
            .style(container::rounded_box);
        pin(opaque(panel))
            .position(menu_position(menu.position, available))
            .into()
    })
    .into()
}

fn menu_position(cursor: Point, available: Size) -> Point {
    Point::new(
        cursor.x.clamp(0.0, (available.width - MENU_WIDTH).max(0.0)),
        cursor
            .y
            .clamp(0.0, (available.height - MENU_HEIGHT).max(0.0)),
    )
}

#[derive(Clone)]
enum HistoryEvent {
    Scrolled { offset_y: f32, content_height: f32 },
    OpenMenu { chat_id: i64, message_id: i64 },
}

// Scrollable translates cursor coordinates for its children. Keep row hit
// testing inside it and attach the original window position to the menu event.
struct History<'a>(Element<'a, HistoryEvent>);

impl Widget<app::Message, Theme, Renderer> for History<'_> {
    fn tag(&self) -> tree::Tag {
        self.0.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.0.as_widget().state()
    }

    fn size(&self) -> Size<Length> {
        self.0.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        self.0.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.0.as_widget().diff(tree);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.0.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.0
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.0
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.0
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, app::Message>,
        viewport: &Rectangle,
    ) {
        let mut events = Vec::new();
        let mut history = Shell::new(&mut events);
        self.0.as_widget_mut().update(
            tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            &mut history,
            viewport,
        );
        shell.merge(history, |message| match message {
            HistoryEvent::Scrolled {
                offset_y,
                content_height,
            } => app::Message::HistoryScrolled {
                offset_y,
                content_height,
            },
            HistoryEvent::OpenMenu {
                chat_id,
                message_id,
            } => cursor
                .position()
                .map_or(app::Message::DismissMessageMenu, |position| {
                    app::Message::OpenMessageMenu {
                        chat_id,
                        message_id,
                        position,
                    }
                }),
        });
    }
}

fn reply_preview<'a, Message: 'a>(
    sender: Option<&str>,
    snippet: impl iced::widget::text::IntoFragment<'a>,
) -> Element<'a, Message> {
    let mut contents = row![text("Reply to").size(12).style(text::primary)].spacing(6);
    if let Some(sender) = sender {
        contents = contents.push(text(format!("{sender}:")).size(12));
    }
    contents = contents.push(
        text(snippet)
            .size(12)
            .width(iced::Length::Fill)
            .wrapping(iced::widget::text::Wrapping::None),
    );
    container(contents)
        .width(iced::Length::Fill)
        .padding([4, 8])
        .style(container::rounded_box)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_stays_inside_the_window() {
        let window = Size::new(800.0, 600.0);
        assert_eq!(
            menu_position(Point::new(200.0, 300.0), window),
            Point::new(200.0, 300.0)
        );
        assert_eq!(
            menu_position(Point::new(799.0, 599.0), window),
            Point::new(660.0, 524.0)
        );
        assert_eq!(
            menu_position(Point::new(10.0, 10.0), Size::new(80.0, 50.0)),
            Point::ORIGIN
        );
    }

    #[test]
    fn right_click_on_scrolled_message_uses_window_coordinates() {
        use iced::{
            Font, Pixels,
            advanced::{
                clipboard,
                widget::operation::scrollable::{AbsoluteOffset, scroll_to},
            },
        };

        let body = iced::widget::column![
            space().height(120),
            mouse_area(space().width(100).height(60)).on_right_press(HistoryEvent::OpenMenu {
                chat_id: 17,
                message_id: 42,
            }),
            space().height(120),
        ];
        let mut history = History(
            scrollable(body)
                .id(app::CHAT_HISTORY_SCROLL_ID)
                .height(Fill)
                .into(),
        );
        let renderer = Renderer::new(Font::default(), Pixels(16.0));
        let size = Size::new(320.0, 100.0);
        let mut tree = Tree::new(&history as &dyn Widget<app::Message, Theme, Renderer>);
        let node = history.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
        let layout = Layout::new(&node);
        history.operate(
            &mut tree,
            layout,
            &renderer,
            &mut scroll_to(
                app::CHAT_HISTORY_SCROLL_ID.into(),
                AbsoluteOffset {
                    x: None,
                    y: Some(120.0),
                },
            ),
        );

        let position = Point::new(20.0, 40.0);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        history.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            layout,
            mouse::Cursor::Available(position),
            &renderer,
            &mut clipboard::Null,
            &mut shell,
            &Rectangle::with_size(size),
        );

        assert!(shell.is_event_captured());
        assert!(messages.iter().any(|message| matches!(
            message,
            app::Message::OpenMessageMenu { chat_id: 17, message_id: 42, position: selected }
                if *selected == position
        )));
    }
}
