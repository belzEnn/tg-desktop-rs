use super::message;
use crate::ui::theme::{appearance, colors, metrics as m};
use crate::{
    app,
    domain::{ChatMessage, ChatSummary, MessageAddress},
};
use iced::{
    Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, renderer,
        widget::{Id, Operation, Tree, operation, tree},
    },
    mouse,
    widget::{center, column, container, scrollable, text},
    window,
};

pub(in crate::ui) fn view<'a>(
    state: &'a app::State,
    open_chat: Option<&'a ChatSummary>,
) -> Element<'a, app::Message> {
    let Some(chat) = open_chat else {
        return center(text("Select a chat to view its messages.").style(appearance::muted_text))
            .style(appearance::conversation)
            .into();
    };

    let mut history = column![]
        .spacing(m::GAP)
        .width(iced::Fill)
        .padding(m::CONVERSATION_PADDING);
    if state.history_loading() {
        history = history.push(text("Loading messages…").style(appearance::muted_text));
    } else if state.messages().is_empty() && state.history_error().is_none() {
        history = history.push(text("No messages in this chat.").style(appearance::muted_text));
    }
    if let Some(error) = state.history_error() {
        history = history.push(text(error).style(appearance::error_text));
    }
    let link = colors::link(state.color_scheme());
    for message in state.messages() {
        history = history.push(message::view(chat, message, link));
    }
    let history = scrollable(history)
        .id(app::CHAT_HISTORY_SCROLL_ID)
        .on_scroll(|viewport| HistoryEvent::Scrolled {
            offset_y: viewport.absolute_offset().y,
            content_height: viewport.content_bounds().height,
        })
        .height(iced::Length::Fill);
    let read = state.read_after(chat).map(|after| VisibleHistory {
        chat_id: chat.id,
        after,
        messages: state.messages(),
    });
    container(Element::new(History {
        content: history.into(),
        read,
    }))
    .width(iced::Fill)
    .height(iced::Fill)
    .style(appearance::conversation)
    .into()
}

#[derive(Clone)]
pub(super) enum HistoryEvent {
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
        // History appends one layout child per message after any status rows.
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

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{
        Fill, Point,
        widget::{mouse_area, space},
    };

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
}
