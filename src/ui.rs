// Iced window and user interface.

use std::borrow::Cow;

use iced::{
    Color, Element, Event, Fill, Font, Length, Point, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, renderer,
        widget::{Id, Operation, Tree, operation, tree},
    },
    font,
    keyboard::{Key, key::Named},
    mouse,
    widget::{
        button, center, column, container, mouse_area, opaque, pin, responsive, rich_text, row,
        scrollable, space, span, stack, text, text_editor, text_input,
    },
    window,
};

use crate::{
    app,
    domain::{
        AuthorizationState, ChatMessage, DeleteScope, MessageAddress, MessagePreview, TextStyle,
        conceal_spoilers, spoiler_mask,
    },
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
                let mut heading = row![text(&chat.title).width(Fill)].spacing(6);
                if chat.unread_count > 0 {
                    heading = heading.push(
                        container(text(chat.unread_count.to_string()).size(12))
                            .padding([2, 6])
                            .style(container::primary),
                    );
                }
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
                                    heading,
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
                    let sender = text(&message.sender_name).size(12);
                    let check = outgoing_check_mark(message, chat.last_read_outbox_message_id);
                    let mut body = if message.is_edited || check.is_some() {
                        let mut heading = row![sender].spacing(8).align_y(iced::Alignment::Center);
                        if message.is_edited {
                            heading = heading.push(text("edited").size(12));
                        }
                        if let Some(check) = check {
                            heading = heading.push(text(check).size(12).style(text::primary));
                        }
                        column![heading]
                    } else {
                        column![sender]
                    }
                    .spacing(3);
                    if let Some(origin) = &message.forwarded_from {
                        body = body.push(text(format!("Forwarded from {origin}")).size(12));
                    }
                    if let Some(reply) = &message.reply {
                        let sender = reply
                            .preview
                            .as_ref()
                            .and_then(|preview| preview.sender_name.as_deref());
                        body = body.push(reply_preview(sender, reply.display_text()));
                    }
                    body = body.push(message_text(chat.id, message));
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
            let editing = state.is_editing();
            let editing_pending = editing && state.mutation_pending();
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
            let read = open_chat.and_then(|chat| {
                state.read_after(chat).map(|after| VisibleHistory {
                    chat_id: chat.id,
                    after,
                    messages: state.messages(),
                })
            });
            let mut conversation = column![Element::new(History {
                content: history.into(),
                read,
            })];
            if open_chat.is_some() {
                if let Some(error) = state.send_error() {
                    conversation = conversation.push(text(error));
                }
                if let Some(selection) = forward_selection {
                    if let Some(source) = state.forward_message() {
                        conversation = conversation.push(
                            text(format!(
                                "Forward message: {}",
                                MessagePreview::message_snippet(&source.text, &source.text_runs)
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
                if state.delete_confirmation().is_none()
                    && let Some(error) = state.message_action_error()
                {
                    conversation = conversation.push(text(error));
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
                            button("Cancel").on_press_maybe(
                                (!editing_pending).then_some(app::Message::CancelEdit)
                            ),
                        ]
                        .align_y(iced::Alignment::Center)
                        .spacing(8),
                    );
                } else if let Some(original) = state.reply_message() {
                    let snippet =
                        MessagePreview::message_snippet(&original.text, &original.text_runs);
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
                    row![
                        composer,
                        button(if editing { "Save" } else { "Send" })
                            .on_press_maybe((!editing_pending).then_some(app::Message::Submit)),
                    ]
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
            if let Some(confirmation) = state.delete_confirmation() {
                layers = layers
                    .push(opaque(
                        mouse_area(space().width(Fill).height(Fill))
                            .on_press(app::Message::CancelDelete),
                    ))
                    .push(delete_confirmation(
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
                    .push(message_menu(
                        menu,
                        !editing,
                        !state.forward_pending(),
                        !editing && !state.mutation_pending(),
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

const MENU_WIDTH: f32 = 152.0;
const MENU_ITEM_HEIGHT: f32 = 30.0;
const MENU_PADDING: f32 = 6.0;
const MENU_SPACING: f32 = 4.0;

fn message_menu(
    menu: app::MessageMenu,
    can_reply: bool,
    can_forward: bool,
    can_change: bool,
) -> Element<'static, app::Message> {
    responsive(move |available| {
        let mut actions = column![
            menu_button(
                "Reply",
                can_reply.then_some(app::Message::SelectReply(menu.message_id))
            ),
            menu_button(
                "Forward",
                can_forward.then_some(app::Message::StartForward(menu.message_id))
            ),
        ]
        .spacing(MENU_SPACING);
        let mut item_count = 2;
        if menu.actions.can_edit {
            actions = actions.push(menu_button(
                "Edit",
                can_change.then_some(app::Message::StartEdit(menu.message_id)),
            ));
            item_count += 1;
        }
        if menu.actions.can_delete_for_self || menu.actions.can_delete_for_everyone {
            actions = actions.push(menu_button(
                "Delete",
                can_change.then_some(app::Message::ShowDeleteConfirmation(menu.message_id)),
            ));
            item_count += 1;
        }
        if menu.request_id.is_some() {
            actions = actions.push(
                container(text("Checking actions…").size(12))
                    .height(MENU_ITEM_HEIGHT)
                    .center_y(MENU_ITEM_HEIGHT),
            );
            item_count += 1;
        }
        let height = MENU_PADDING * 2.0
            + MENU_ITEM_HEIGHT * item_count as f32
            + MENU_SPACING * (item_count - 1) as f32;
        let panel = container(actions)
            .width(MENU_WIDTH)
            .height(height)
            .padding(MENU_PADDING)
            .style(container::rounded_box);
        pin(opaque(panel))
            .position(menu_position(menu.position, available, height))
            .into()
    })
    .into()
}

fn menu_button(
    label: &'static str,
    action: Option<app::Message>,
) -> iced::widget::Button<'static, app::Message> {
    button(label)
        .width(Fill)
        .height(MENU_ITEM_HEIGHT)
        .on_press_maybe(action)
}

fn menu_position(cursor: Point, available: Size, height: f32) -> Point {
    Point::new(
        cursor.x.clamp(0.0, (available.width - MENU_WIDTH).max(0.0)),
        cursor.y.clamp(0.0, (available.height - height).max(0.0)),
    )
}

fn delete_confirmation<'a>(
    confirmation: &app::DeleteConfirmation,
    original: Option<&ChatMessage>,
    pending: bool,
    error: Option<&'a str>,
) -> Element<'a, app::Message> {
    let mut actions = row![
        button(if pending { "Close" } else { "Cancel" }).on_press(app::Message::CancelDelete),
    ]
    .spacing(8);
    if confirmation.actions.can_delete_for_self {
        actions = actions.push(
            button("Delete for me")
                .style(button::danger)
                .on_press_maybe(
                    (!pending).then_some(app::Message::ConfirmDelete(DeleteScope::ForSelf)),
                ),
        );
    }
    if confirmation.actions.can_delete_for_everyone {
        actions = actions.push(
            button("Delete for everyone")
                .style(button::danger)
                .on_press_maybe(
                    (!pending).then_some(app::Message::ConfirmDelete(DeleteScope::ForEveryone)),
                ),
        );
    }
    let mut dialog = column![
        text(if pending {
            "Deleting message…"
        } else {
            "Delete this message?"
        })
        .size(18),
    ]
    .spacing(12);
    if let Some(original) = original {
        dialog = dialog.push(
            text(
                MessagePreview::message_snippet(&original.text, &original.text_runs).into_string(),
            )
            .size(13),
        );
    }
    dialog = dialog.push(actions);
    if let Some(error) = error {
        dialog = dialog.push(text(error));
    }
    center(opaque(
        container(dialog).padding(16).style(container::rounded_box),
    ))
    .into()
}

#[derive(Clone)]
enum HistoryEvent {
    Scrolled { offset_y: f32, content_height: f32 },
    OpenMenu { chat_id: i64, message_id: i64 },
    RevealSpoilers(MessageAddress),
}

// Scrollable translates cursor coordinates for its children. Keep row hit
// testing inside it and attach the original window position to the menu event.
struct History<'a> {
    content: Element<'a, HistoryEvent>,
    read: Option<VisibleHistory<'a>>,
}

struct VisibleHistory<'a> {
    chat_id: i64,
    after: i64,
    messages: &'a [ChatMessage],
}

#[derive(Default)]
struct ScrollTranslation(Vector);

impl Operation for ScrollTranslation {
    fn traverse(&mut self, _: &mut dyn FnMut(&mut dyn Operation)) {}

    fn scrollable(
        &mut self,
        _: Option<&Id>,
        _: Rectangle,
        _: Rectangle,
        translation: Vector,
        _: &mut dyn operation::Scrollable,
    ) {
        self.0 = translation;
    }
}

impl VisibleHistory<'_> {
    fn boundary(
        &self,
        content: Layout<'_>,
        viewport: Rectangle,
        scroll_y: f32,
    ) -> Option<MessageAddress> {
        // History appends one layout child per message after the header/status rows.
        let first_message = content.children().len().checked_sub(self.messages.len())?;
        let row_bottom = |index| {
            let row = content.child(first_message + index).bounds();
            row.y + row.height - scroll_y
        };
        let bottom = viewport.y + viewport.height;

        // Find the first row that extends below the viewport, then scan upward
        // through the visible rows. Older pages need no extra geometry cache.
        let (mut start, mut end) = (0, self.messages.len());
        while start < end {
            let middle = start + (end - start) / 2;
            if row_bottom(middle) <= bottom {
                start = middle + 1;
            } else {
                end = middle;
            }
        }
        let message_id = (0..start)
            .rev()
            .take_while(|&index| row_bottom(index) > viewport.y)
            .map(|index| &self.messages[index])
            .filter(|message| {
                message.can_reference && !message.is_outgoing && message.id > self.after
            })
            .map(|message| message.id)
            .max()?;

        Some(MessageAddress {
            chat_id: self.chat_id,
            message_id,
        })
    }
}

impl Widget<app::Message, Theme, Renderer> for History<'_> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
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
        self.content
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
        self.content
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
        self.content.as_widget_mut().update(
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
            HistoryEvent::RevealSpoilers(address) => app::Message::RevealSpoilers(address),
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
        if matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && let Some(read) = &self.read
            && let Some(visible) = layout.bounds().intersection(viewport)
            && let Some(content) = layout.children().next()
        {
            let mut scroll = ScrollTranslation::default();
            self.content
                .as_widget_mut()
                .operate(tree, layout, renderer, &mut scroll);
            if let Some(address) = read.boundary(content, visible, scroll.0.y) {
                shell.publish(app::Message::HistoryReadBoundary(address));
            }
        }
    }
}

fn message_text(chat_id: i64, message: &ChatMessage) -> Element<'_, HistoryEvent> {
    let plain = || {
        let content = if message.spoilers_revealed {
            Cow::Borrowed(message.text.as_str())
        } else {
            conceal_spoilers(&message.text, &message.text_runs)
        };
        let widget = match content {
            Cow::Borrowed(fragment) => text(fragment),
            Cow::Owned(fragment) => text(fragment),
        };
        widget.wrapping(text::Wrapping::Word).into()
    };
    if message.text_runs.is_empty() {
        return plain();
    }

    let mut start = 0;
    let mut spans = Vec::with_capacity(message.text_runs.len());
    let mut hidden = false;
    for run in &message.text_runs {
        let Some(fragment) = message.text.get(start..run.end) else {
            return plain();
        };
        if !message.spoilers_revealed && run.style.contains(TextStyle::SPOILER) {
            hidden = true;
            spans.push(spoiler_span(fragment));
        } else {
            spans.push(message_span(fragment, run.style));
        }
        start = run.end;
    }
    if start != message.text.len() {
        return plain();
    }
    let rich = rich_text(spans).wrapping(text::Wrapping::Word);
    if hidden {
        rich.on_link_click(move |()| {
            HistoryEvent::RevealSpoilers(MessageAddress {
                chat_id,
                message_id: message.id,
            })
        })
        .into()
    } else {
        rich.into()
    }
}

fn spoiler_span(fragment: &str) -> text::Span<'_, (), Font> {
    span(spoiler_mask(fragment))
        .background(Color::from_rgba(0.48, 0.50, 0.54, 0.24))
        .link(())
}

fn message_span(fragment: &str, style: TextStyle) -> text::Span<'_, (), Font> {
    let bold = style.contains(TextStyle::BOLD);
    let italic = style.contains(TextStyle::ITALIC);
    let pre = style.contains(TextStyle::PRE);
    let monospace = pre || style.contains(TextStyle::CODE);
    let link = style.contains(TextStyle::LINK);
    let mut segment = span(fragment)
        .underline(link || style.contains(TextStyle::UNDERLINE))
        .strikethrough(style.contains(TextStyle::STRIKETHROUGH));
    if bold || italic || monospace {
        let mut font = if monospace {
            Font::MONOSPACE
        } else {
            Font::DEFAULT
        };
        if bold {
            font.weight = font::Weight::Bold;
        }
        if italic {
            font.style = font::Style::Italic;
        }
        segment = segment.font(font);
    }
    if monospace {
        let opacity = if pre { 0.24 } else { 0.16 };
        segment = segment.background(Color::from_rgba(0.48, 0.50, 0.54, opacity));
    }
    if link {
        segment = segment.color(Color::from_rgb(0.26, 0.47, 0.75));
    }
    segment
}

fn outgoing_check_mark(message: &ChatMessage, read_through: Option<i64>) -> Option<&'static str> {
    let read_through = read_through?;
    if message.is_outgoing && message.can_reference && message.id > 0 {
        return Some(if message.id <= read_through {
            "✓✓"
        } else {
            "✓"
        });
    }
    None
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
            menu_position(Point::new(200.0, 300.0), window, 76.0),
            Point::new(200.0, 300.0)
        );
        assert_eq!(
            menu_position(Point::new(799.0, 599.0), window, 144.0),
            Point::new(648.0, 456.0)
        );
        assert_eq!(
            menu_position(Point::new(10.0, 10.0), Size::new(80.0, 50.0), 76.0),
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
        let mut history = History {
            content: scrollable(body)
                .id(app::CHAT_HISTORY_SCROLL_ID)
                .height(Fill)
                .into(),
            read: None,
        };
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

    #[test]
    fn read_boundary_uses_the_scrolled_viewport_and_excludes_incomplete_rows() {
        use iced::{
            Font, Pixels,
            advanced::{
                clipboard,
                widget::operation::scrollable::{AbsoluteOffset, scroll_to},
            },
        };
        use std::time::Instant;

        let messages = [1, 2, 3, 4].map(|id| ChatMessage {
            id,
            date: 0,
            sender_name: String::new(),
            text: String::new(),
            text_runs: Box::default(),
            is_outgoing: id == 3,
            can_reference: true,
            is_text: true,
            is_edited: false,
            spoilers_revealed: false,
            reply: None,
            forwarded_from: None,
        });
        let body = iced::widget::column![
            space().height(20),
            space().height(60),
            space().height(60),
            space().height(40),
            space().height(100),
        ]
        .width(Fill);
        let mut history = History {
            content: scrollable(body)
                .id(app::CHAT_HISTORY_SCROLL_ID)
                .height(Fill)
                .into(),
            read: Some(VisibleHistory {
                chat_id: 17,
                after: 0,
                messages: &messages,
            }),
        };
        let renderer = Renderer::new(Font::default(), Pixels(16.0));
        let size = Size::new(320.0, 100.0);
        let mut tree = Tree::new(&history as &dyn Widget<app::Message, Theme, Renderer>);
        let node = history.layout(&mut tree, &renderer, &layout::Limits::new(Size::ZERO, size));
        let layout = Layout::new(&node);

        for (offset, expected) in [(0.0, Some(1)), (80.0, Some(2)), (150.0, None)] {
            history.operate(
                &mut tree,
                layout,
                &renderer,
                &mut scroll_to(
                    app::CHAT_HISTORY_SCROLL_ID.into(),
                    AbsoluteOffset {
                        x: None,
                        y: Some(offset),
                    },
                ),
            );
            let mut events = Vec::new();
            history.update(
                &mut tree,
                &Event::Window(window::Event::RedrawRequested(Instant::now())),
                layout,
                mouse::Cursor::Unavailable,
                &renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut events),
                &Rectangle::with_size(size),
            );
            assert_eq!(
                events.iter().find_map(|message| match message {
                    app::Message::HistoryReadBoundary(MessageAddress {
                        chat_id: 17,
                        message_id,
                    }) => Some(*message_id),
                    _ => None,
                }),
                expected,
            );
        }
    }

    #[test]
    fn only_confirmed_outgoing_messages_in_private_chats_show_checks() {
        let mut message = ChatMessage {
            id: 42,
            date: 0,
            sender_name: "You".into(),
            text: "Hello".into(),
            text_runs: Box::default(),
            is_outgoing: true,
            can_reference: true,
            is_text: true,
            is_edited: false,
            spoilers_revealed: false,
            reply: None,
            forwarded_from: None,
        };
        assert_eq!(outgoing_check_mark(&message, Some(41)), Some("✓"));
        assert_eq!(outgoing_check_mark(&message, Some(42)), Some("✓✓"));
        assert_eq!(outgoing_check_mark(&message, Some(43)), Some("✓✓"));
        assert_eq!(outgoing_check_mark(&message, None), None);
        message.can_reference = false;
        assert_eq!(outgoing_check_mark(&message, Some(43)), None);
        message.can_reference = true;
        message.is_outgoing = false;
        assert_eq!(outgoing_check_mark(&message, Some(43)), None);
    }

    #[test]
    fn span_keeps_preformatted_whitespace_and_combines_visual_styles() {
        let formatted = message_span("a\n  b", TextStyle::PRE.union(TextStyle::BOLD));
        let font = formatted.font.unwrap();
        assert_eq!(font.family, Font::MONOSPACE.family);
        assert_eq!(font.weight, font::Weight::Bold);
        assert_eq!(formatted.text.as_ref(), "a\n  b");
        assert!(formatted.highlight.is_some());

        let link = message_span(
            "link",
            TextStyle::LINK
                .union(TextStyle::ITALIC)
                .union(TextStyle::STRIKETHROUGH),
        );
        assert!(link.underline && link.strikethrough && link.color.is_some());
        assert_eq!(link.font.unwrap().style, font::Style::Italic);
    }

    #[test]
    fn concealed_span_handles_a_click_without_exposing_its_text() {
        use crate::domain::TextRun;
        use iced::{Pixels, advanced::clipboard};

        let hidden = spoiler_span("🙂 x");
        assert_eq!(hidden.text.as_ref(), "• •");
        let message = ChatMessage {
            id: 42,
            date: 0,
            sender_name: String::new(),
            text: "secret🙂".into(),
            text_runs: Box::new([TextRun {
                end: "secret🙂".len(),
                style: TextStyle::SPOILER,
            }]),
            is_outgoing: false,
            can_reference: true,
            is_text: true,
            is_edited: false,
            spoilers_revealed: false,
            reply: None,
            forwarded_from: None,
        };
        let mut content = message_text(17, &message);
        let renderer = Renderer::new(Font::default(), Pixels(16.0));
        let mut tree = Tree::new(&content);
        let size = Size::new(320.0, 100.0);
        let node = content.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, size),
        );
        let mut events = Vec::new();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            content.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(event),
                Layout::new(&node),
                mouse::Cursor::Available(Point::new(5.0, 5.0)),
                &renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut events),
                &Rectangle::with_size(size),
            );
        }
        assert!(matches!(
            events.as_slice(),
            [HistoryEvent::RevealSpoilers(MessageAddress {
                chat_id: 17,
                message_id: 42,
            })]
        ));
    }
}
