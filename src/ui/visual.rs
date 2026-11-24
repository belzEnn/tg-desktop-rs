use std::{
    fs::{File, create_dir_all},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use iced::{
    Event, Rectangle, Renderer, Size,
    advanced::{
        Layout, Shell, clipboard,
        layout::Limits,
        renderer::{Headless, Style},
        widget::{
            Tree,
            operation::scrollable::{RelativeOffset, snap_to},
        },
    },
    mouse, window,
};

use crate::{
    app,
    config::ColorScheme,
    domain::{ChatMessage, ChatSummary, MessagePreview, MessageReply, TextRun, TextStyle},
};

use super::{m, theme, view};

const WIDE_SIZE: Size = Size::new(1920.0, 1024.0);

#[test]
fn workspace_fits_supported_sizes_and_keeps_history_and_composer_visible() {
    let mut state = fixture(ColorScheme::Light);
    drop(app::update(&mut state, app::Message::ToggleTheme));
    assert!(matches!(state.color_scheme(), ColorScheme::Dark));

    let renderer = renderer();
    let mut element = view(&state);
    let mut tree = Tree::new(&element);
    for size in [m::WINDOW_SIZE, m::MIN_WINDOW_SIZE, WIDE_SIZE] {
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
        let layout = Layout::new(&node);
        let panes = layout.child(0).child(0);
        let sidebar = panes.child(0).bounds();
        let conversation = panes.child(2);
        let header = conversation.child(0).bounds();
        let history = conversation.child(1).bounds();
        let composer = conversation.child(2).bounds();
        assert_eq!(layout.bounds().size(), size);
        assert_eq!(sidebar.width, m::sidebar_width(size.width));
        assert!(sidebar.x + sidebar.width <= header.x);
        assert_eq!(header.y + header.height, history.y);
        assert_eq!(history.y + history.height, composer.y);
        assert!(history.height > 0.0 && composer.height > 0.0);
        let scrollable = conversation.child(1).child(0).bounds();
        assert_eq!(scrollable.y + scrollable.height, composer.y);
        if size == WIDE_SIZE {
            assert!(scrollable.y > history.y);
        }
        assert_eq!(composer.y + composer.height, size.height);
        assert_eq!(composer.x + composer.width, size.width);
    }
}

#[test]
#[ignore = "writes screenshot artifacts to UI_BASELINE_DIR"]
fn capture_visual_baseline() {
    let directory =
        PathBuf::from(std::env::var_os("UI_BASELINE_DIR").expect("set UI_BASELINE_DIR"));
    create_dir_all(&directory).unwrap();
    for (label, scheme) in [("light", ColorScheme::Light), ("dark", ColorScheme::Dark)] {
        let state = fixture(scheme);
        for (variant, size) in [
            ("default", m::WINDOW_SIZE),
            ("minimum", m::MIN_WINDOW_SIZE),
            ("wide", WIDE_SIZE),
        ] {
            let path = directory.join(format!("{label}-{variant}.ppm"));
            capture(&path, &state, size).unwrap();
        }
    }
}

fn renderer() -> Renderer {
    Renderer::new(
        theme::typography::BODY_FONT,
        iced::Settings::default().default_text_size,
    )
}

fn capture(path: &Path, state: &app::State, size: Size) -> std::io::Result<()> {
    let theme = theme::iced_theme(state.color_scheme());
    let mut renderer = renderer();
    let mut element = view(state);
    let mut tree = Tree::new(&element);
    let node = element
        .as_widget_mut()
        .layout(&mut tree, &renderer, &Limits::new(Size::ZERO, size));
    let layout = Layout::new(&node);
    let viewport = Rectangle::with_size(size);
    element.as_widget_mut().operate(
        &mut tree,
        layout,
        &renderer,
        &mut snap_to(
            app::CHAT_HISTORY_SCROLL_ID.into(),
            RelativeOffset::END.into(),
        ),
    );
    // iced resolves visual states on a redraw event before painting.
    element.as_widget_mut().update(
        &mut tree,
        &Event::Window(window::Event::RedrawRequested(Instant::now())),
        layout,
        mouse::Cursor::Unavailable,
        &renderer,
        &mut clipboard::Null,
        &mut Shell::new(&mut Vec::new()),
        &viewport,
    );
    element.as_widget().draw(
        &tree,
        &mut renderer,
        &theme,
        &Style {
            text_color: theme.palette().text,
        },
        layout,
        mouse::Cursor::Unavailable,
        &viewport,
    );
    let physical = Size::new(size.width as u32, size.height as u32);
    let pixels = renderer.screenshot(physical, 1.0, theme.palette().background);
    assert_eq!(
        pixels.len(),
        physical.width as usize * physical.height as usize * 4
    );

    let mut file = BufWriter::new(File::create(path)?);
    write!(file, "P6\n{} {}\n255\n", physical.width, physical.height)?;
    for pixel in pixels.as_chunks::<4>().0 {
        file.write_all(&pixel[..3])?;
    }
    file.flush()
}

fn fixture(scheme: ColorScheme) -> app::State {
    let chats = [
        ("Weekend plans", "I’ll bring a map and a thermos.", 0),
        ("Studio notes", "Updated checklist for the afternoon", 3),
        ("Reading group", "The new chapter is ready.", 12),
        (
            "Local cycling routes and repairs",
            "Meet beside the east entrance.\nBring a spare tube.",
            1,
        ),
        ("Drafts", "No messages yet", 0),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (title, preview, unread_count))| ChatSummary {
        id: index as i64 + 1,
        title: title.into(),
        last_message_id: (index != 4).then_some(if index == 0 { 9 } else { 6 }),
        last_message_date: (index != 4).then_some(1_761_819_600),
        last_message_is_outgoing: index == 1,
        last_message_preview: preview.into(),
        unread_count,
        last_read_inbox_message_id: 6,
        last_read_outbox_message_id: Some(4),
        position: 10 - index as i64,
        can_receive_forwards: true,
    })
    .collect();

    let mut messages = vec![
        message(
            1,
            "Marta",
            "Does Saturday morning work? We can take the trail along the river.",
            false,
        ),
        message(
            2,
            "You",
            "Yes, let’s meet at 09:30.\nI’ll meet you by the footbridge.",
            true,
        ),
        message(
            3,
            "Marta",
            "The marked trail stays open. Details: river trail map",
            false,
        ),
        message(4, "You", "I’ll bring a map and a thermos.", true),
        message(5, "Marta", "[Photo]", false),
        message(6, "You", "I’ll check the forecast in the morning.", true),
        message(7, "Marta", "Sounds good.", false),
        message(8, "Marta", "See you!", false),
        message(9, "Marta", "I’ll text you when I leave.", false),
    ];
    messages[2].text_runs = Box::new([
        TextRun {
            end: "The marked trail".len(),
            style: TextStyle::BOLD,
        },
        TextRun {
            end: "The marked trail stays open. Details: ".len(),
            style: TextStyle::default(),
        },
        TextRun {
            end: messages[2].text.len(),
            style: TextStyle::LINK,
        },
    ]);
    messages[3].reply = Some(Box::new(MessageReply {
        chat_id: 1,
        message_id: 1,
        quote: None,
        preview: Some(MessagePreview::from_text("Marta", &messages[0].text, &[])),
    }));
    messages[4].forwarded_from = Some("Trail notices".into());
    messages[4].is_text = false;
    messages[5].is_edited = true;
    app::visual_fixture(scheme, chats, messages)
}

fn message(id: i64, sender: &str, content: &str, outgoing: bool) -> ChatMessage {
    ChatMessage {
        id,
        date: 1_761_819_240 + id as i32 * 60,
        sender_name: sender.into(),
        text: content.into(),
        text_runs: Box::default(),
        is_outgoing: outgoing,
        can_reference: true,
        is_text: true,
        is_edited: false,
        spoilers_revealed: false,
        reply: None,
        forwarded_from: None,
    }
}
