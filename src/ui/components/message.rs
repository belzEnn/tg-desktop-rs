use super::history::HistoryEvent;
use crate::domain::{
    ChatMessage, ChatSummary, MessageAddress, TextStyle, conceal_spoilers, spoiler_mask,
};
use crate::ui::theme::{colors, metrics as m, typography as ty};
use iced::{
    Element, Font, font,
    widget::{column, container, mouse_area, rich_text, row, span, text},
};
use std::borrow::Cow;

pub(super) fn view<'a>(chat: &ChatSummary, message: &'a ChatMessage) -> Element<'a, HistoryEvent> {
    let sender = text(&message.sender_name).size(ty::CAPTION);
    let check = outgoing_check_mark(message, chat.last_read_outbox_message_id);
    let mut body = if message.is_edited || check.is_some() {
        let mut heading = row![sender]
            .spacing(m::GAP)
            .align_y(iced::Alignment::Center);
        if message.is_edited {
            heading = heading.push(text("edited").size(ty::CAPTION));
        }
        if let Some(check) = check {
            heading = heading.push(text(check).size(ty::CAPTION).style(text::primary));
        }
        column![heading]
    } else {
        column![sender]
    }
    .spacing(m::GAP_COMPACT);
    if let Some(origin) = &message.forwarded_from {
        body = body.push(text(format!("Forwarded from {origin}")).size(ty::CAPTION));
    }
    if let Some(reply) = &message.reply {
        let sender = reply
            .preview
            .as_ref()
            .and_then(|preview| preview.sender_name.as_deref());
        body = body.push(reply_preview(sender, reply.display_text()));
    }
    body = body.push(message_text(chat.id, message));
    let content = container(body).padding(m::MESSAGE_PADDING);
    if message.can_reference {
        mouse_area(content)
            .on_right_press(HistoryEvent::OpenMenu {
                chat_id: chat.id,
                message_id: message.id,
            })
            .into()
    } else {
        content.into()
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
        .background(colors::HIGHLIGHT)
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
        segment = segment.color(colors::LINK);
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

pub(super) fn reply_preview<'a, Message: 'a>(
    sender: Option<&str>,
    snippet: impl iced::widget::text::IntoFragment<'a>,
) -> Element<'a, Message> {
    let mut contents =
        row![text("Reply to").size(ty::CAPTION).style(text::primary)].spacing(m::GAP_INLINE);
    if let Some(sender) = sender {
        contents = contents.push(text(format!("{sender}:")).size(ty::CAPTION));
    }
    contents = contents.push(
        text(snippet)
            .size(ty::CAPTION)
            .width(iced::Length::Fill)
            .wrapping(iced::widget::text::Wrapping::None),
    );
    container(contents)
        .width(iced::Length::Fill)
        .padding(m::QUOTE_PADDING)
        .style(container::rounded_box)
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
