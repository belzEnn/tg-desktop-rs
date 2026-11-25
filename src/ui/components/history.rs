use std::collections::{HashMap, HashSet};

use super::message;
use crate::ui::theme::{appearance, colors, metrics as m};
use crate::{
    app::{self, HistoryScrollDirection},
    domain::{ChatMessage, ChatSummary, MessageAddress},
};
use iced::{
    Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Clipboard, Layout, Shell, Widget, layout, renderer,
        widget::{Id, Operation, Tree, operation, tree},
    },
    mouse,
    widget::{Column, center, column, container, scrollable, space, text},
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

    container(Element::new(History {
        content: scrollable(column![]).into(),
        read: None,
        source: Some(HistorySource { state, chat }),
        geometry: Geometry::default(),
    }))
    .width(iced::Fill)
    .height(iced::Fill)
    .align_y(if state.messages().is_empty() {
        iced::alignment::Vertical::Top
    } else {
        iced::alignment::Vertical::Bottom
    })
    .style(appearance::conversation)
    .into()
}

fn continues_group(previous: &ChatMessage, current: &ChatMessage, private_chat: bool) -> bool {
    const GROUP_INTERVAL_SECONDS: i64 = 120;
    private_chat
        && previous.can_reference
        && current.can_reference
        && previous.is_text
        && current.is_text
        && previous.is_outgoing == current.is_outgoing
        && previous.date > 0
        && previous.date / 86_400 == current.date / 86_400
        && (0..=GROUP_INTERVAL_SECONDS)
            .contains(&(i64::from(current.date) - i64::from(previous.date)))
}

#[derive(Clone)]
pub(super) enum HistoryEvent {
    OpenMenu { chat_id: i64, message_id: i64 },
    RevealSpoilers(MessageAddress),
}

// Scrollable translates cursor coordinates for its children. Keep row hit
// testing inside it and attach the original window position to the menu event.
struct History<'a> {
    content: Element<'a, HistoryEvent>,
    read: Option<VisibleHistory<'a>>,
    source: Option<HistorySource<'a>>,
    geometry: Geometry,
}

#[derive(Clone, Copy)]
struct HistorySource<'a> {
    state: &'a app::State,
    chat: &'a ChatSummary,
}

#[derive(Default)]
struct HistoryViewport {
    chat_id: Option<i64>,
    width: f32,
    heights: HashMap<i64, RowMeasurement>,
    anchor: Option<RowAnchor>,
    history_end_revision: u64,
    reported: Option<(f32, f32, f32)>,
    scroll_direction: Option<HistoryScrollDirection>,
    visible_ids: Vec<i64>,
    has_status: bool,
}

struct RowMeasurement {
    signature: u64,
    height: f32,
}

#[derive(Clone, Copy)]
struct RowAnchor {
    id: i64,
    index: usize,
    inset: f32,
}

#[derive(Default)]
struct Geometry {
    offsets: Vec<f32>,
    translation: f32,
    extent: f32,
}

impl Geometry {
    fn anchor_at(&self, messages: &[ChatMessage], offset: f32) -> Option<RowAnchor> {
        let index = self
            .offsets
            .partition_point(|&top| top <= offset)
            .saturating_sub(1)
            .min(messages.len().saturating_sub(1));
        Some(RowAnchor {
            id: messages.get(index)?.id,
            index,
            inset: offset - self.offsets.get(index)?,
        })
    }
}

impl<'a> HistorySource<'a> {
    fn status(&self) -> Option<Element<'a, HistoryEvent>> {
        if let Some(error) = self.state.history_error() {
            Some(text(error).style(appearance::error_text).into())
        } else if self.state.history_loading() {
            Some(
                text("Loading messages…")
                    .style(appearance::muted_text)
                    .into(),
            )
        } else if self.state.loading_older_history() {
            Some(
                text("Loading older messages…")
                    .style(appearance::muted_text)
                    .into(),
            )
        } else if self.state.messages().is_empty() {
            Some(
                text("No messages in this chat.")
                    .style(appearance::muted_text)
                    .into(),
            )
        } else {
            None
        }
    }
}

impl<'a> History<'a> {
    fn layout_rows(
        &mut self,
        source: HistorySource<'a>,
        state: &mut HistoryViewport,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> (f32, bool) {
        const ESTIMATED_ROW_HEIGHT: f32 = 48.0;
        const MEASURED_ROW_LIMIT: usize = 1024;
        let padding = f32::from(m::CONVERSATION_PADDING);
        let width = (limits.max().width - 2.0 * padding).max(0.0);
        let height = limits.max().height;
        let history_end_revision = source.state.history_end_revision();
        if state.chat_id != Some(source.chat.id) {
            *state = HistoryViewport {
                chat_id: Some(source.chat.id),
                history_end_revision,
                ..HistoryViewport::default()
            };
        } else if state.history_end_revision != history_end_revision {
            state.history_end_revision = history_end_revision;
            state.anchor = None;
        }
        if state.width != width {
            state.width = width;
            state.heights.clear();
        }
        let messages = source.state.messages();
        if messages.is_empty() {
            state.anchor = None;
        }
        let mut status = source.status();
        let status_height = status
            .as_mut()
            .map_or(0.0, |status| measure_height(status, renderer, width));
        let private_chat = source.chat.last_read_outbox_message_id.is_some();
        let link = colors::link(source.state.color_scheme());
        let mut measure = |index: usize| {
            let entry = &messages[index];
            let grouped = index
                .checked_sub(1)
                .is_some_and(|previous| continues_group(&messages[previous], entry, private_chat));
            let mut row = message::view(source.chat, entry, link, grouped);
            let signature = message::layout_signature(source.chat, entry, grouped);
            let measurement = state
                .heights
                .entry(entry.id)
                .or_insert_with(|| RowMeasurement {
                    signature,
                    height: measure_height(&mut row, renderer, width),
                });
            if measurement.signature != signature {
                *measurement = RowMeasurement {
                    signature,
                    height: measure_height(&mut row, renderer, width),
                };
            }
            (entry.id, row, measurement.height)
        };

        // A scrolled history is anchored to a message and an intra-row offset.
        // At the newest edge, work backwards so the latest message stays visible.
        let mut start = state.anchor.map_or(messages.len(), |anchor| {
            messages
                .iter()
                .position(|message| message.id == anchor.id)
                .unwrap_or(anchor.index.min(messages.len().saturating_sub(1)))
        });
        let anchor_index = start;
        let mut end = start;
        let mut rows = Vec::new();
        let mut inset = state.anchor.map_or(0.0, |anchor| anchor.inset);
        let mut remaining = height;
        while end < messages.len() && (remaining > 0.0 || rows.is_empty()) {
            let row = measure(end);
            if end == anchor_index {
                // A width or content change can shorten the anchored row.
                // Clamp before deciding how many following rows to build.
                inset = inset.min((row.2 - 1.0).max(0.0));
                remaining += inset;
                state.anchor = Some(RowAnchor {
                    id: row.0,
                    index: end,
                    inset,
                });
            }
            remaining -= row.2;
            rows.push(row);
            end += 1;
        }
        if end == messages.len() {
            remaining -= padding;
        }
        // The viewport can hit the newest edge after a resize or deletion.
        // Fill any room above the original anchor before clamping the scroll.
        let mut preceding = Vec::new();
        while start > 0 && remaining > 0.0 && end == messages.len() {
            start -= 1;
            let row = measure(start);
            remaining -= row.2;
            preceding.push(row);
        }

        let row_origin = padding + status_height;
        let offsets = &mut self.geometry.offsets;
        offsets.clear();
        offsets.push(row_origin);
        for message in messages {
            let height = state
                .heights
                .get(&message.id)
                .map_or(ESTIMATED_ROW_HEIGHT, |row| row.height);
            offsets.push(offsets.last().copied().unwrap_or(row_origin) + height);
        }
        if state.heights.len() > MEASURED_ROW_LIMIT {
            let spare = MEASURED_ROW_LIMIT.saturating_sub(end - start);
            let keep_start = start
                .saturating_sub(spare / 2)
                .min(messages.len().saturating_sub(MEASURED_ROW_LIMIT));
            let keep_end = (keep_start + MEASURED_ROW_LIMIT).min(messages.len());
            let kept: HashSet<_> = messages[keep_start..keep_end]
                .iter()
                .map(|message| message.id)
                .collect();
            state.heights.retain(|id, _| kept.contains(id));
        }
        let has_status = status.is_some();
        let visible_ids = messages[start..end].iter().map(|message| message.id);
        let rows_changed = state.has_status != has_status
            || state.visible_ids.iter().copied().ne(visible_ids.clone());
        if rows_changed {
            state.has_status = has_status;
            state.visible_ids.clear();
            state.visible_ids.extend(visible_ids);
        }
        // Keep both spacer nodes even when their heights are zero.
        let mut children = Vec::with_capacity(end - start + 2 + usize::from(has_status));
        children.extend(status);
        children.push(space().height(offsets[start] - row_origin).into());
        children.extend(
            preceding
                .into_iter()
                .rev()
                .chain(rows)
                .map(|(_, row, _)| row),
        );
        children.push(
            space()
                .height(offsets[messages.len()] - offsets[end])
                .into(),
        );
        let history = Column::from_vec(children)
            .width(iced::Fill)
            .padding(m::CONVERSATION_PADDING);
        self.content = scrollable(history)
            .width(iced::Fill)
            .id(app::CHAT_HISTORY_SCROLL_ID)
            .height(iced::Shrink)
            .into();
        self.read = source
            .state
            .read_after(source.chat)
            .map(|after| VisibleHistory {
                chat_id: source.chat.id,
                after,
                first_message: 1 + usize::from(has_status),
                messages: &messages[start..end],
            });
        let target = if state.anchor.is_some() {
            offsets[anchor_index] + inset
        } else {
            f32::INFINITY
        };
        (target, rows_changed)
    }

    fn scroll_translation(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        target: Option<f32>,
    ) -> ScrollTranslation {
        let mut scroll = ScrollTranslation {
            target,
            ..ScrollTranslation::default()
        };
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, &mut scroll);
        scroll
    }
}

fn measure_height(element: &mut Element<'_, HistoryEvent>, renderer: &Renderer, width: f32) -> f32 {
    let mut tree = Tree::new(&*element);
    let limits = layout::Limits::new(Size::ZERO, Size::new(width, f32::INFINITY));
    element
        .as_widget_mut()
        .layout(&mut tree, renderer, &limits)
        .size()
        .height
}

struct VisibleHistory<'a> {
    chat_id: i64,
    after: i64,
    first_message: usize,
    messages: &'a [ChatMessage],
}

#[derive(Default)]
struct ScrollTranslation {
    value: Vector,
    extent: f32,
    viewport_height: f32,
    target: Option<f32>,
}

impl Operation for ScrollTranslation {
    fn traverse(&mut self, children: &mut dyn FnMut(&mut dyn Operation)) {
        if self.extent == 0.0 {
            children(self);
        }
    }

    fn scrollable(
        &mut self,
        _: Option<&Id>,
        bounds: Rectangle,
        content: Rectangle,
        translation: Vector,
        state: &mut dyn operation::Scrollable,
    ) {
        self.extent = content.height;
        self.viewport_height = bounds.height;
        self.value = translation;
        if let Some(target) = self.target {
            let offset = target.clamp(0.0, (content.height - bounds.height).max(0.0));
            state.scroll_to(operation::scrollable::AbsoluteOffset {
                x: None,
                y: Some(offset),
            });
            // Iced rounds the translation it reports for the applied offset.
            self.value.y = offset.round();
        }
    }
}

impl VisibleHistory<'_> {
    fn boundary(
        &self,
        content: Layout<'_>,
        viewport: Rectangle,
        scroll_y: f32,
    ) -> Option<MessageAddress> {
        let row_bottom = |index| {
            let row = content.child(self.first_message + index).bounds();
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
        tree::Tag::of::<HistoryViewport>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(HistoryViewport::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(iced::Fill, iced::Shrink)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, _tree: &mut Tree) {
        // The scrollable's rows are reconciled once their viewport is known.
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let target = if let Some(source) = self.source {
            let (target, rows_changed) =
                self.layout_rows(source, tree.state.downcast_mut(), renderer, limits);
            if rows_changed {
                // A recycled row must not inherit another message's mouse state.
                tree.children[0].children.clear();
            }
            Some(target)
        } else {
            None
        };
        tree.children[0].diff(&self.content);
        let node = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        if target.is_some() {
            let scroll = self.scroll_translation(tree, Layout::new(&node), renderer, target);
            self.geometry.translation = scroll.value.y;
            self.geometry.extent = scroll.extent;
        }
        node
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
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
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
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            &mut history,
            viewport,
        );
        shell.merge(history, |message| match message {
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
        let scroll = self.scroll_translation(tree, layout, renderer, None);
        if let Some(source) = &self.source {
            let state = tree.state.downcast_mut::<HistoryViewport>();
            if scroll.value.y != self.geometry.translation {
                let max_offset = (scroll.extent - scroll.viewport_height).max(0.0).round();
                state.anchor = if scroll.value.y < max_offset {
                    self.geometry
                        .anchor_at(source.state.messages(), scroll.value.y)
                } else {
                    None
                };
                state.scroll_direction = Some(if scroll.value.y < self.geometry.translation {
                    HistoryScrollDirection::Up
                } else {
                    HistoryScrollDirection::Down
                });
                shell.invalidate_layout();
                shell.request_redraw();
                return;
            }
            // At the top edge another upward gesture may retry a failed page
            // even when the scroll offset cannot change.
            let wheel_at_edge = cursor.is_over(layout.bounds())
                && matches!(event,
                    Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. }
                    }) if *y > 0.0
                )
                && scroll.value.y <= scroll.viewport_height;
            let direction = wheel_at_edge
                .then_some(HistoryScrollDirection::Up)
                .or(state.scroll_direction.take());
            let geometry = (
                self.geometry.translation,
                self.geometry.extent,
                scroll.viewport_height,
            );
            if state.reported != Some(geometry) || direction.is_some() {
                state.reported = Some(geometry);
                shell.publish(app::Message::HistoryScrolled {
                    chat_id: source.chat.id,
                    offset_y: geometry.0,
                    viewport_height: geometry.2,
                    direction,
                });
            }
        }
        if matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && let Some(read) = &self.read
            && let Some(visible) = layout.bounds().intersection(viewport)
            && let Some(content) = layout.children().next()
            && let Some(address) = read.boundary(content, visible, scroll.value.y)
        {
            shell.publish(app::Message::HistoryReadBoundary(address));
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
    fn large_history_keeps_its_anchor_on_prepend_and_jumps_to_end_on_send() {
        use crate::domain::BackendEvent;
        use iced::{Font, Pixels, advanced::clipboard, widget::text_editor};
        use std::time::Instant;

        let chat = ChatSummary {
            id: 17,
            title: "History".into(),
            last_message_id: None,
            last_message_date: None,
            last_message_is_outgoing: false,
            last_message_preview: String::new(),
            unread_count: 0,
            last_read_inbox_message_id: 0,
            last_read_outbox_message_id: Some(0),
            position: 1,
            can_receive_forwards: true,
        };
        let message = |id| ChatMessage {
            id,
            date: 0,
            sender_name: String::new(),
            text: if id == 550 {
                "Tall message\n".repeat(250)
            } else if id % 2 == 0 {
                "One line".into()
            } else {
                "First line\nSecond line\nThird line".into()
            },
            text_runs: Box::default(),
            is_outgoing: false,
            can_reference: true,
            is_text: true,
            is_edited: false,
            spoilers_revealed: false,
            reply: None,
            forwarded_from: None,
        };
        let mut app = app::visual_fixture(
            crate::config::ColorScheme::Light,
            vec![chat.clone()],
            (51..=550).map(message).collect(),
        );
        let renderer = Renderer::new(Font::default(), Pixels(16.0));
        let size = Size::new(320.0, 300.0);
        let limits = layout::Limits::new(Size::ZERO, size);
        let mut widget = view(&app, Some(&chat));
        let mut tree = Tree::new(&widget);
        let node = widget.as_widget_mut().layout(&mut tree, &renderer, &limits);
        let scroll = Layout::new(&node).child(0);
        assert!(scroll.child(0).children().len() < 15);

        let mut events = Vec::new();
        let mut shell = Shell::new(&mut events);
        widget.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y: 500.0 },
            }),
            Layout::new(&node),
            mouse::Cursor::Available(Point::new(20.0, 40.0)),
            &renderer,
            &mut clipboard::Null,
            &mut shell,
            &Rectangle::with_size(size),
        );
        assert!(shell.is_layout_invalid());
        let anchor = tree.state.downcast_ref::<HistoryViewport>().anchor.unwrap();
        assert_eq!(anchor.id, 550);
        assert!(anchor.inset > 1000.0);
        drop(widget);

        drop(app::update(
            &mut app,
            app::Message::BackendEvent(BackendEvent::MessageContentChanged {
                address: MessageAddress {
                    chat_id: chat.id,
                    message_id: 550,
                },
                text: "Shortened message".into(),
                text_runs: Box::default(),
                is_text: true,
            }),
        ));
        let mut widget = view(&app, Some(&chat));
        tree.diff(&widget);
        let node = widget.as_widget_mut().layout(&mut tree, &renderer, &limits);
        let scroll = Layout::new(&node).child(0);
        assert!(scroll.child(0).children().len() < 15);

        let offset = 2300.0;
        widget.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut ScrollTranslation {
                target: Some(offset),
                ..ScrollTranslation::default()
            },
        );
        widget.as_widget_mut().update(
            &mut tree,
            &Event::Window(window::Event::RedrawRequested(Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut Vec::new()),
            &Rectangle::with_size(size),
        );
        let node = widget.as_widget_mut().layout(&mut tree, &renderer, &limits);
        let scroll = Layout::new(&node).child(0);
        assert!(scroll.child(0).children().len() < 15);
        let before = tree.state.downcast_ref::<HistoryViewport>().anchor.unwrap();
        let (id, inset) = (before.id, before.inset);
        let before_node = scroll.child(0).child(1).bounds();
        let mut before_scroll = ScrollTranslation::default();
        widget.as_widget_mut().operate(
            &mut tree,
            Layout::new(&node),
            &renderer,
            &mut before_scroll,
        );
        drop(widget);

        app::visual_history_page(&mut app, (1..=50).map(message).collect());
        let mut widget = view(&app, Some(&chat));
        tree.diff(&widget);
        let node = widget.as_widget_mut().layout(&mut tree, &renderer, &limits);
        let scroll = Layout::new(&node).child(0);
        assert!(scroll.child(0).children().len() < 15);
        let after = tree.state.downcast_ref::<HistoryViewport>().anchor.unwrap();
        assert_eq!((after.id, after.inset), (id, inset));
        let mut after_scroll = ScrollTranslation::default();
        widget
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut after_scroll);
        let after_node = scroll.child(0).child(1).bounds();
        assert!(
            ((before_node.y - before_scroll.value.y) - (after_node.y - after_scroll.value.y)).abs()
                < 0.01
        );

        let narrower = layout::Limits::new(Size::ZERO, Size::new(250.0, size.height));
        let node = widget
            .as_widget_mut()
            .layout(&mut tree, &renderer, &narrower);
        let scroll = Layout::new(&node).child(0);
        assert!(scroll.child(0).children().len() < 15);
        assert_eq!(
            tree.state
                .downcast_ref::<HistoryViewport>()
                .anchor
                .unwrap()
                .id,
            id
        );
        drop(widget);

        drop(app::update(
            &mut app,
            app::Message::ComposerAction(text_editor::Action::Edit(text_editor::Edit::Paste(
                std::sync::Arc::new("New message".into()),
            ))),
        ));
        drop(app::update(&mut app, app::Message::Submit));
        let sent = app.messages().last().unwrap();
        assert_eq!(sent.text, "New message");

        let mut widget = view(&app, Some(&chat));
        tree.diff(&widget);
        let node = widget
            .as_widget_mut()
            .layout(&mut tree, &renderer, &narrower);
        let viewport = tree.state.downcast_ref::<HistoryViewport>();
        assert!(viewport.anchor.is_none());
        assert_eq!(viewport.visible_ids.last(), Some(&sent.id));

        let mut scroll = ScrollTranslation::default();
        widget
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut scroll);
        assert!(scroll.value.y > 0.0);
        assert_eq!(
            scroll.value.y,
            (scroll.extent - scroll.viewport_height).round()
        );

        let mut events = Vec::new();
        let mut shell = Shell::new(&mut events);
        widget.as_widget_mut().update(
            &mut tree,
            &Event::Window(window::Event::RedrawRequested(Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut clipboard::Null,
            &mut shell,
            &Rectangle::with_size(narrower.max()),
        );
        assert!(!shell.is_layout_invalid());
        assert!(
            tree.state
                .downcast_ref::<HistoryViewport>()
                .anchor
                .is_none()
        );
    }

    #[test]
    fn tight_spacing_applies_only_to_consecutive_private_text_messages() {
        let mut previous = ChatMessage {
            id: 1,
            date: 86_400 + 60,
            sender_name: String::new(),
            text: "Hello".into(),
            text_runs: Box::default(),
            is_outgoing: false,
            can_reference: true,
            is_text: true,
            is_edited: false,
            spoilers_revealed: false,
            reply: None,
            forwarded_from: None,
        };
        let mut current = previous.clone();
        current.id = 2;
        current.date += 35;
        assert!(continues_group(&previous, &current, true));
        assert!(!continues_group(&previous, &current, false));
        current.is_outgoing = true;
        assert!(!continues_group(&previous, &current, true));
        current.is_outgoing = false;
        current.date += 120;
        assert!(!continues_group(&previous, &current, true));
        previous.date = 2 * 86_400 - 30;
        current.date = previous.date + 35;
        assert!(!continues_group(&previous, &current, true));
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
            source: None,
            geometry: Geometry::default(),
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
                first_message: 1,
                messages: &messages,
            }),
            source: None,
            geometry: Geometry::default(),
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
