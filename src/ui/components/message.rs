use super::{delivery_mark, history::HistoryEvent, time_label};
use crate::domain::{
    ChatMessage, ChatSummary, MessageAddress, TextRun, TextStyle, conceal_spoilers, spoiler_mask,
};
use crate::ui::theme::{appearance, colors, metrics as m, typography as ty};
use iced::{
    Color, Element, Fill, Font,
    alignment::Horizontal,
    font,
    widget::{column, container, mouse_area, rich_text, row, span, text},
};
use std::{
    borrow::Cow,
    hash::{DefaultHasher, Hash, Hasher},
};

pub(super) fn layout_signature(chat: &ChatSummary, message: &ChatMessage, grouped: bool) -> u64 {
    let mut hash = DefaultHasher::new();
    grouped.hash(&mut hash);
    chat.last_read_outbox_message_id.is_some().hash(&mut hash);
    message.sender_name.hash(&mut hash);
    message.text.hash(&mut hash);
    message.text_runs.hash(&mut hash);
    message.spoilers_revealed.hash(&mut hash);
    message.is_outgoing.hash(&mut hash);
    message.is_text.hash(&mut hash);
    message.is_edited.hash(&mut hash);
    message.date.hash(&mut hash);
    message.forwarded_from.hash(&mut hash);
    outgoing_check_mark(message, chat.last_read_outbox_message_id).hash(&mut hash);
    message.reply.as_deref().map(reply_text).hash(&mut hash);
    hash.finish()
}

pub(super) fn view<'a>(
    chat: &ChatSummary,
    message: &'a ChatMessage,
    link: Color,
    grouped: bool,
) -> Element<'a, HistoryEvent> {
    let address = MessageAddress {
        chat_id: chat.id,
        message_id: message.id,
    };
    let content = if message.is_text {
        Content::Text {
            text: &message.text,
            runs: &message.text_runs,
            spoilers_revealed: message.spoilers_revealed,
        }
    } else {
        Content::Placeholder(&message.text)
    };
    let outgoing = message.is_outgoing;
    let mut body = column![].spacing(m::GAP_TINY);
    if !outgoing && chat.last_read_outbox_message_id.is_none() && !message.sender_name.is_empty() {
        body = body.push(
            text(&message.sender_name)
                .size(ty::CAPTION)
                .font(ty::HEADING_FONT),
        );
    }
    if let Some(origin) = &message.forwarded_from {
        body = body.push(
            text(format!("Forwarded from {origin}"))
                .size(ty::CAPTION)
                .style(appearance::muted_text),
        );
    }
    if let Some(reply) = &message.reply {
        let (sender, snippet) = reply_text(reply);
        body = body.push(reply_preview(sender, snippet, iced::Shrink));
    }
    let content = content.view(address, link);
    let check = outgoing_check_mark(message, chat.last_read_outbox_message_id);
    if message.date > 0 || message.is_edited || check.is_some() {
        body = body.push(
            row![content, metadata(message, check)]
                .spacing(m::GAP_INLINE)
                .align_y(iced::alignment::Vertical::Bottom)
                .wrap()
                .vertical_spacing(m::GAP_TINY),
        );
    } else {
        body = body.push(content);
    }
    let bubble = container(body)
        .style(move |theme| appearance::bubble(theme, outgoing))
        .padding(m::MESSAGE_PADDING)
        .max_width(m::MESSAGE_MAX_WIDTH);
    let bubble = if message.can_reference {
        mouse_area(bubble)
            .on_right_press(HistoryEvent::OpenMenu {
                chat_id: address.chat_id,
                message_id: address.message_id,
            })
            .into()
    } else {
        Element::from(bubble)
    };
    let alignment = if outgoing {
        Horizontal::Right
    } else {
        Horizontal::Left
    };
    // Each message owns one variable-height child in the history column.
    let gap = if grouped {
        m::MESSAGE_GROUP_GAP
    } else {
        m::MESSAGE_GAP
    };
    container(bubble)
        .padding(iced::Padding::ZERO.top(gap))
        .width(Fill)
        .align_x(alignment)
        .into()
}

fn metadata<'a>(message: &ChatMessage, check: Option<&'static str>) -> Element<'a, HistoryEvent> {
    let prefix = if message.is_edited { "edited " } else { "" };
    let suffix = match check {
        Some("✓✓") => " ✓✓",
        Some(_) => " ✓",
        None => "",
    };
    let date = (message.date > 0).then_some(message.date);
    text(time_label(date, prefix, suffix))
        .size(ty::MESSAGE_METADATA)
        .style(appearance::muted_text)
        .into()
}

// Match displayable content here. A future media view can reuse the row,
// interaction and metadata once the domain exposes its data.
enum Content<'a> {
    Text {
        text: &'a str,
        runs: &'a [TextRun],
        spoilers_revealed: bool,
    },
    Placeholder(&'a str),
}

impl<'a> Content<'a> {
    fn view(self, address: MessageAddress, link: Color) -> Element<'a, HistoryEvent> {
        match self {
            Self::Text {
                text,
                runs,
                spoilers_revealed,
            } => message_text(address, text, runs, spoilers_revealed, link),
            Self::Placeholder(label) => text(label)
                .size(ty::MESSAGE_SIZE)
                .line_height(ty::MESSAGE_LINE_HEIGHT)
                .style(appearance::muted_text)
                .wrapping(text::Wrapping::WordOrGlyph)
                .into(),
        }
    }
}

fn message_text<'a>(
    address: MessageAddress,
    text_value: &'a str,
    runs: &'a [TextRun],
    spoilers_revealed: bool,
    link: Color,
) -> Element<'a, HistoryEvent> {
    let plain = || {
        let content = if spoilers_revealed {
            Cow::Borrowed(text_value)
        } else {
            conceal_spoilers(text_value, runs)
        };
        let widget = match content {
            Cow::Borrowed(fragment) => text(fragment),
            Cow::Owned(fragment) => text(fragment),
        };
        widget
            .size(ty::MESSAGE_SIZE)
            .line_height(ty::MESSAGE_LINE_HEIGHT)
            .wrapping(text::Wrapping::WordOrGlyph)
            .into()
    };
    if runs.is_empty() {
        return plain();
    }

    let mut start = 0;
    let mut spans = Vec::with_capacity(runs.len());
    let mut hidden = false;
    for run in runs {
        let Some(fragment) = text_value.get(start..run.end) else {
            return plain();
        };
        if !spoilers_revealed && run.style.contains(TextStyle::SPOILER) {
            hidden = true;
            spans.push(spoiler_span(fragment));
        } else {
            spans.push(message_span(fragment, run.style, link));
        }
        start = run.end;
    }
    if start != text_value.len() {
        return plain();
    }
    let rich = rich_text(spans)
        .size(ty::MESSAGE_SIZE)
        .line_height(ty::MESSAGE_LINE_HEIGHT)
        .wrapping(text::Wrapping::WordOrGlyph);
    if hidden {
        rich.on_link_click(move |()| HistoryEvent::RevealSpoilers(address))
            .into()
    } else {
        rich.into()
    }
}

fn spoiler_span(fragment: &str) -> text::Span<'_, (), Font> {
    span(spoiler_mask(fragment))
        .background(colors::HIGHLIGHT)
        .link(())
}

fn message_span(fragment: &str, style: TextStyle, link_color: Color) -> text::Span<'_, (), Font> {
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
            ty::CODE_FONT
        } else {
            ty::BODY_FONT
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
        segment = segment.background(if pre { colors::HIGHLIGHT } else { colors::CODE });
    }
    if link {
        segment = segment.color(link_color);
    }
    segment
}

fn outgoing_check_mark(message: &ChatMessage, read_through: Option<i64>) -> Option<&'static str> {
    if message.is_outgoing && message.can_reference {
        delivery_mark(message.id, read_through)
    } else {
        None
    }
}

fn reply_text(reply: &crate::domain::MessageReply) -> (Option<&str>, &str) {
    let sender = reply
        .preview
        .as_ref()
        .and_then(|preview| preview.sender_name.as_deref());
    (sender, reply.display_text())
}

pub(super) fn reply_preview<'a, Message: 'a>(
    sender: Option<&str>,
    snippet: impl iced::widget::text::IntoFragment<'a>,
    width: iced::Length,
) -> Element<'a, Message> {
    let label = sender.map_or(Cow::Borrowed("Reply"), |sender| {
        Cow::Owned(format!("Reply to {sender}"))
    });
    let contents = column![
        text(label)
            .size(ty::CAPTION)
            .style(text::primary)
            .wrapping(text::Wrapping::None),
        text(snippet)
            .size(ty::CAPTION)
            .wrapping(text::Wrapping::None),
    ]
    .spacing(m::GAP_TINY);
    container(container(contents).width(width).clip(true))
        .width(width)
        .padding(m::QUOTE_PADDING)
        .style(appearance::quote)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{
        Event, Point, Rectangle, Renderer, Size,
        advanced::{Layout, Shell, layout, widget::Tree},
        mouse,
    };

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
        let formatted = message_span(
            "a\n  b",
            TextStyle::PRE.union(TextStyle::BOLD),
            Color::BLACK,
        );
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
            Color::BLACK,
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
        let mut content = message_text(
            MessageAddress {
                chat_id: 17,
                message_id: message.id,
            },
            &message.text,
            &message.text_runs,
            message.spoilers_revealed,
            Color::BLACK,
        );
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
