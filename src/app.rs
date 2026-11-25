// Application state and update logic.

use iced::widget::text_editor;
use iced::{
    Event, Point, Subscription, Task, event,
    keyboard::{self, key::Named},
    mouse, window,
};
use std::collections::{HashMap, HashSet};

use crate::{
    domain::{
        AuthorizationState, BackendEvent, ChatMessage, ChatPageAnchor, ChatPageQuery, ChatSummary,
        DeleteScope, HistoryEdge, MAX_PENDING_SENDS, MessageAction, MessageActions, MessageAddress,
        MessagePreview, MessageReply, TextRun, TextStyle,
    },
    tg::{Adapter, AdapterCommandError},
};

const REPLY_LOOKUP_BATCH_SIZE: usize = 4;
const CHAT_PAGE_SIZE: usize = 50;
const CHAT_CACHE_LIMIT: usize = 150;
pub(crate) const HISTORY_CACHE_LIMIT: usize = 250;
pub(crate) const CHAT_LIST_SCROLL_ID: &str = "main-chats";
pub(crate) const CHAT_HISTORY_SCROLL_ID: &str = "chat-history";
pub(crate) const CHAT_COMPOSER_ID: &str = "chat-composer";

/// State owned by the application's update loop.
pub(crate) struct State {
    color_scheme: crate::config::ColorScheme,
    backend: Adapter,
    authorization_state: Option<AuthorizationState>,
    phone: String,
    code: String,
    password: String,
    authorization_error: Option<String>,
    logout_pending: bool,
    chats: Vec<ChatSummary>,
    chat_pages: ChatPages,
    open_chat_id: Option<i64>,
    open_chat_summary: Option<ChatSummary>,
    forward_chat_summary: Option<ChatSummary>,
    window_focused: bool,
    history_read: HistoryRead,
    read_in_flight: Option<PendingRead>,
    messages: Vec<ChatMessage>,
    history_pins: Vec<ChatMessage>,
    history_loading: bool,
    history_error: Option<String>,
    has_older_messages: bool,
    has_newer_messages: bool,
    history_request: Option<(u64, HistoryEdge)>,
    history_failed: Option<HistoryEdge>,
    history_viewport: Option<HistoryViewport>,
    history_end_revision: u64,
    pending_sends: HashMap<i64, Option<i64>>,
    composer: text_editor::Content,
    next_temporary_message_id: i64,
    send_error: Option<String>,
    message_menu: Option<MessageMenu>,
    edit: Option<EditSession>,
    delete_confirmation: Option<DeleteConfirmation>,
    pending_mutation: Option<PendingMutation>,
    next_action_request_id: u64,
    message_action_error: Option<String>,
    reply_to: Option<i64>,
    reply_lookup_pending: bool,
    forward_selection: Option<ForwardSelection>,
    forward_pending: bool,
    forward_status: Option<String>,
}

/// Distances to the edges of the active history viewport.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HistoryViewport {
    pub chat_id: i64,
    pub offset_y: f32,
    pub remaining_y: f32,
    pub height: f32,
    pub direction: Option<HistoryScrollDirection>,
}

impl HistoryViewport {
    fn near(self, edge: HistoryEdge) -> bool {
        self.height > 0.0
            && match edge {
                HistoryEdge::Older => self.offset_y <= self.height,
                HistoryEdge::Newer => self.remaining_y <= self.height,
            }
    }
}

/// The message and window position of an open context menu.
#[derive(Clone, Copy)]
pub(crate) struct MessageMenu {
    pub message_id: i64,
    pub position: Point,
    pub actions: MessageActions,
    pub request_id: Option<u64>,
}

#[derive(Default)]
struct ChatPages {
    started: bool,
    first_offset: usize,
    next_offset: usize,
    after: Option<i64>,
    finished: bool,
    pending: Option<(ChatPageQuery, ChatPageKind)>,
    viewport: Option<ChatListViewport>,
    retry: Option<ChatPageKind>,
    refresh_required: bool,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChatPageKind {
    Next,
    Previous,
    Refresh,
}

/// The first visible sidebar row and its position in the viewport.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ChatListViewport {
    pub anchor: Option<i64>,
    pub inset: f32,
    pub row_height: f32,
    pub near_start: bool,
    pub near_end: bool,
}

#[derive(Default)]
struct HistoryRead {
    viewed_through: i64,
    accepted_through: i64,
    failed_through: i64,
}

struct PendingRead {
    request_id: u64,
    address: MessageAddress,
}

struct PendingMutation {
    request_id: u64,
    address: MessageAddress,
}

struct EditSession {
    message_id: i64,
    draft: text_editor::Content,
}

/// A message selected for deletion and the available scopes.
pub(crate) struct DeleteConfirmation {
    pub message_id: i64,
    pub actions: MessageActions,
}

/// The selected source message and optional destination of a forward.
pub(crate) struct ForwardSelection {
    pub message_id: i64,
    pub destination: Option<i64>,
}

/// Direction of a history scroll gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryScrollDirection {
    Up,
    Down,
}

/// Messages handled by the application update loop.
#[derive(Debug, Clone)]
pub(crate) enum Message {
    BackendEvent(BackendEvent),
    ToggleTheme,
    Initialized(Result<(), AdapterCommandError>),
    PhoneChanged(String),
    CodeChanged(String),
    PasswordChanged(String),
    SubmitPhone,
    SubmitCode,
    SubmitPassword,
    AuthorizationCommandSent(Result<(), AdapterCommandError>),
    LogOut,
    LogoutCommandSent(Result<(), AdapterCommandError>),
    OpenChat(i64),
    ChatsScrolled(ChatListViewport),
    RetryChatPage,
    ChatPageQueued {
        request_id: u64,
        result: Result<(), AdapterCommandError>,
    },
    WindowFocusChanged(bool),
    HistoryReadBoundary(MessageAddress),
    ReadReceiptQueued {
        request_id: u64,
        result: Result<(), AdapterCommandError>,
    },
    ChatOpenCommandSent(Result<(), AdapterCommandError>),
    HistoryScrolled(HistoryViewport),
    HistoryPageQueued {
        request_id: u64,
        result: Result<(), AdapterCommandError>,
    },
    ComposerAction(text_editor::Action),
    OpenMessageMenu {
        chat_id: i64,
        message_id: i64,
        position: Point,
    },
    DismissMessageMenu,
    RevealSpoilers(MessageAddress),
    Escape,
    StartEdit(i64),
    CancelEdit,
    ShowDeleteConfirmation(i64),
    ConfirmDelete(DeleteScope),
    CancelDelete,
    ActionQueued {
        request_id: u64,
        result: Result<(), AdapterCommandError>,
    },
    SelectReply(i64),
    CancelReply,
    ReplyLookupQueued(Result<(), AdapterCommandError>),
    StartForward(i64),
    SelectForwardDestination(i64),
    ConfirmForward,
    CancelForward,
    ForwardQueued(Result<(), AdapterCommandError>),
    Submit,
    SendMessageCommandSent {
        chat_id: i64,
        message_id: i64,
        text: String,
        result: Result<(), AdapterCommandError>,
    },
}

/// Creates application state and starts the backend through an iced task.
pub(crate) fn boot(
    backend: Adapter,
    color_scheme: crate::config::ColorScheme,
) -> (State, Task<Message>) {
    let state = State {
        color_scheme,
        backend: backend.clone(),
        authorization_state: None,
        phone: String::new(),
        code: String::new(),
        password: String::new(),
        authorization_error: None,
        logout_pending: false,
        chats: Vec::new(),
        chat_pages: ChatPages::default(),
        open_chat_id: None,
        open_chat_summary: None,
        forward_chat_summary: None,
        window_focused: false,
        history_read: HistoryRead::default(),
        read_in_flight: None,
        messages: Vec::new(),
        history_pins: Vec::new(),
        history_loading: false,
        history_error: None,
        has_older_messages: false,
        has_newer_messages: false,
        history_request: None,
        history_failed: None,
        history_viewport: None,
        history_end_revision: 0,
        pending_sends: HashMap::new(),
        composer: text_editor::Content::new(),
        next_temporary_message_id: -1,
        send_error: None,
        message_menu: None,
        edit: None,
        delete_confirmation: None,
        pending_mutation: None,
        next_action_request_id: 0,
        message_action_error: None,
        reply_to: None,
        reply_lookup_pending: false,
        forward_selection: None,
        forward_pending: false,
        forward_status: None,
    };
    let task = Task::perform(
        async move { backend.initialize().await },
        Message::Initialized,
    );
    (state, task)
}

/// Subscribes the application to backend events.
pub(crate) fn subscription(state: &State) -> Subscription<Message> {
    let events = Subscription::batch([
        state.backend.subscription().map(Message::BackendEvent),
        event::listen_with(window_focus_event),
    ]);
    if state.message_menu.is_some() || state.edit.is_some() || state.delete_confirmation.is_some() {
        Subscription::batch([events, event::listen_with(message_menu_event)])
    } else {
        events
    }
}

fn window_focus_event(event: Event, _: event::Status, _: window::Id) -> Option<Message> {
    match event {
        Event::Window(window::Event::Focused) => Some(Message::WindowFocusChanged(true)),
        Event::Window(window::Event::Unfocused) => Some(Message::WindowFocusChanged(false)),
        _ => None,
    }
}

fn message_menu_event(event: Event, status: event::Status, _: window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::Escape),
            ..
        }) => Some(Message::Escape),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
            if status == event::Status::Ignored =>
        {
            Some(Message::DismissMessageMenu)
        }
        Event::Mouse(mouse::Event::WheelScrolled { .. })
        | Event::Window(window::Event::Resized(_)) => Some(Message::DismissMessageMenu),
        _ => None,
    }
}

/// Applies backend events and command results to application state.
pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::ToggleTheme => {
            use crate::config::ColorScheme;
            state.color_scheme = match state.color_scheme {
                ColorScheme::Light => ColorScheme::Dark,
                ColorScheme::Dark => ColorScheme::Light,
            };
        }
        Message::BackendEvent(BackendEvent::AuthorizationChanged(authorization_state)) => {
            if authorization_state == AuthorizationState::Closed {
                state.authorization_state = None;
                state.authorization_error = None;
                state.logout_pending = false;
                state.chats.clear();
                state.chat_pages = ChatPages::default();
                state.open_chat_id = None;
                state.open_chat_summary = None;
                state.history_read = HistoryRead::default();
                state.read_in_flight = None;
                state.message_menu = None;
                state.edit = None;
                state.delete_confirmation = None;
                state.pending_mutation = None;
                state.message_action_error = None;
                state.messages.clear();
                state.history_pins.clear();
                state.pending_sends.clear();
                state.has_older_messages = false;
                state.has_newer_messages = false;
                state.history_request = None;
                state.history_failed = None;
                state.history_viewport = None;
                state.composer = text_editor::Content::new();
                state.send_error = None;
                state.reply_to = None;
                state.forward_selection = None;
                state.forward_chat_summary = None;
                state.forward_pending = false;
                state.forward_status = None;
                let backend = state.backend.clone();
                return Task::perform(
                    async move { backend.initialize().await },
                    Message::Initialized,
                );
            }
            state.authorization_state = Some(authorization_state);
            state.authorization_error = None;
            if authorization_state != AuthorizationState::WaitPhoneNumber {
                state.phone.clear();
            }
            if authorization_state != AuthorizationState::WaitCode {
                state.code.clear();
            }
            if authorization_state != AuthorizationState::WaitPassword {
                state.password.clear();
            }
            tracing::info!(
                state = authorization_state.as_str(),
                "Telegram authorization state changed"
            );
            if authorization_state == AuthorizationState::Ready && !state.chat_pages.started {
                return request_next_chat_page(state);
            }
        }
        Message::BackendEvent(BackendEvent::AuthorizationError(error)) => {
            state.authorization_error = Some(error);
            state.password.clear();
            state.logout_pending = false;
        }
        Message::BackendEvent(BackendEvent::ChatUpdated(chat)) => {
            update_pinned_chat(state, &chat);
            if let Some(existing) = state
                .chats
                .iter_mut()
                .find(|existing| existing.id == chat.id)
            {
                update_chat_summary(existing, chat);
                sort_chats(&mut state.chats);
            } else if state.chat_pages.started {
                add_live_chat(state, chat);
            }
        }
        Message::BackendEvent(BackendEvent::ChatPageLoaded {
            request_id,
            offset,
            next_offset,
            next_after,
            has_more,
            chats,
        }) => {
            if let Some((_, kind)) = state
                .chat_pages
                .pending
                .filter(|(query, _)| query.request_id == request_id)
            {
                state.chat_pages.pending = None;
                state.chat_pages.error = None;
                state.chat_pages.retry = None;
                let viewport = state.chat_pages.viewport;
                if state.chats.len() + chats.len() > CHAT_CACHE_LIMIT
                    && viewport.is_some_and(|viewport| match kind {
                        ChatPageKind::Next => !viewport.near_end,
                        ChatPageKind::Previous => !viewport.near_start,
                        ChatPageKind::Refresh => false,
                    })
                {
                    return refresh_pending_chat_pages(state);
                }
                match kind {
                    ChatPageKind::Next => {
                        if state.chats.is_empty() {
                            state.chat_pages.first_offset = offset;
                        }
                        state.chat_pages.next_offset = next_offset;
                        state.chat_pages.after = next_after;
                        state.chat_pages.finished = !has_more;
                    }
                    ChatPageKind::Previous => {
                        state.chat_pages.first_offset = offset;
                    }
                    ChatPageKind::Refresh => {
                        state.chat_pages.first_offset = offset;
                        state.chat_pages.next_offset = next_offset;
                        state.chat_pages.after = next_after;
                        state.chat_pages.finished &= !has_more;
                        state
                            .chats
                            .retain(|chat| chats.iter().any(|loaded| loaded.id == chat.id));
                    }
                }
                for chat in chats {
                    if let Some(existing) = state
                        .chats
                        .iter_mut()
                        .find(|existing| existing.id == chat.id)
                    {
                        // Keep the live summary when an adjacent page overlaps.
                        if kind == ChatPageKind::Refresh {
                            update_chat_summary(existing, chat);
                        }
                    } else {
                        state.chats.push(chat);
                    }
                }
                if kind == ChatPageKind::Refresh {
                    for chat in [
                        &mut state.open_chat_summary,
                        &mut state.forward_chat_summary,
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if let Some(updated) = state.chats.iter().find(|entry| entry.id == chat.id)
                        {
                            update_chat_summary(chat, updated.clone());
                        }
                    }
                }
                sort_chats(&mut state.chats);
                trim_chats(state, kind);
                let follow_up = if state.chat_pages.refresh_required {
                    refresh_chat_pages(state)
                } else if state.chats.len() < CHAT_PAGE_SIZE
                    && has_more
                    && kind == ChatPageKind::Next
                {
                    request_next_chat_page(state)
                } else {
                    Task::none()
                };
                return Task::batch([restore_chat_scroll(state, viewport), follow_up]);
            }
        }
        Message::BackendEvent(BackendEvent::ChatPageFailed {
            request_id,
            message,
        }) => {
            fail_chat_page(state, request_id, message);
        }
        Message::BackendEvent(BackendEvent::ChatListInvalidated) => {
            state.chat_pages.refresh_required = true;
            return refresh_chat_pages(state);
        }
        Message::BackendEvent(BackendEvent::ChatInboxRead {
            chat_id,
            last_read_message_id,
            unread_count,
        }) => {
            for chat in state.chat_entries_mut(chat_id) {
                if last_read_message_id >= chat.last_read_inbox_message_id {
                    chat.last_read_inbox_message_id = last_read_message_id;
                    chat.unread_count = unread_count;
                }
            }
        }
        Message::BackendEvent(BackendEvent::ChatOutboxRead {
            chat_id,
            last_read_message_id,
        }) => {
            for chat in state.chat_entries_mut(chat_id) {
                if let Some(read) = &mut chat.last_read_outbox_message_id {
                    *read = (*read).max(last_read_message_id);
                }
            }
        }
        Message::BackendEvent(BackendEvent::ReadReceiptFinished {
            request_id,
            accepted,
        }) => {
            return finish_read_receipt(state, request_id, accepted);
        }
        Message::BackendEvent(BackendEvent::ChatRemoved(chat_id)) => {
            let before = state.chats.len();
            state.chats.retain(|chat| chat.id != chat_id);
            if state.chats.len() < before
                && state.chat_pages.next_offset > state.chat_pages.first_offset
            {
                state.chat_pages.next_offset -= 1;
            }
            if state
                .forward_chat_summary
                .as_ref()
                .is_some_and(|chat| chat.id == chat_id)
            {
                state.forward_chat_summary = None;
                if let Some(selection) = &mut state.forward_selection {
                    selection.destination = None;
                }
            }
        }
        Message::BackendEvent(BackendEvent::ChatHistoryLoaded {
            chat_id,
            messages,
            has_older_messages,
        }) => {
            if state.open_chat_id == Some(chat_id) {
                state.message_menu = None;
                merge_history_page(&mut state.messages, messages);
                state.history_loading = false;
                state.history_error = None;
                state.has_older_messages = has_older_messages;
                state.has_newer_messages = false;
                state.history_request = None;
                state.history_failed = None;
                state.history_viewport = None;
                trim_history(state, HistoryEdge::Newer);
                return refresh_reply_previews(state);
            }
        }
        Message::BackendEvent(BackendEvent::HistoryPageLoaded {
            request_id,
            messages,
            has_more,
        }) => {
            if let Some((_, edge)) = state.history_request.filter(|(id, _)| *id == request_id) {
                state.history_request = None;
                state.history_error = None;
                // If the user moved across the window while the request was in
                // flight, leave that viewport intact and reload the page later.
                if state.messages.len() + messages.len() > HISTORY_CACHE_LIMIT
                    && state
                        .history_viewport
                        .is_some_and(|viewport| !viewport.near(edge))
                {
                    return Task::none();
                }
                match edge {
                    HistoryEdge::Older => state.has_older_messages = has_more,
                    HistoryEdge::Newer => state.has_newer_messages = has_more,
                }
                if !messages.is_empty() {
                    state
                        .history_pins
                        .retain(|pin| messages.iter().all(|message| message.id != pin.id));
                    merge_history_page(&mut state.messages, messages);
                    trim_history(state, edge);
                    return refresh_reply_previews(state);
                }
            }
        }
        Message::BackendEvent(BackendEvent::HistoryPageFailed {
            request_id,
            message,
        }) => {
            fail_history_page(state, request_id, message);
        }
        Message::BackendEvent(BackendEvent::ChatHistoryError { chat_id, message }) => {
            if state.open_chat_id == Some(chat_id) {
                state.history_loading = false;
                state.history_error = Some(message);
            }
        }
        Message::BackendEvent(BackendEvent::NewMessage { chat_id, message }) => {
            if state.open_chat_id == Some(chat_id) {
                insert_live_message(state, message);
                return refresh_reply_previews(state);
            }
        }
        Message::BackendEvent(BackendEvent::MessageSendSucceeded {
            chat_id,
            old_message_id,
            local_message_id,
            message,
        }) => {
            if state.open_chat_id == Some(chat_id) {
                if let Some(local_id) = local_message_id {
                    state.pending_sends.remove(&local_id);
                }
                state.messages.retain(|existing| {
                    existing.id != old_message_id && Some(existing.id) != local_message_id
                });
                insert_live_message(state, message);
                return refresh_reply_previews(state);
            }
        }
        Message::BackendEvent(BackendEvent::MessageSendRejected {
            chat_id,
            message_id,
            text,
            reason,
        }) => {
            if state.open_chat_id == Some(chat_id) {
                restore_rejected_message(state, message_id, &text);
                state.send_error = Some(reason);
            }
        }
        Message::BackendEvent(BackendEvent::MessageActionsLoaded { request_id, result }) => {
            apply_message_actions(state, request_id, result);
        }
        Message::BackendEvent(BackendEvent::MessageMutationFinished { request_id, result }) => {
            finish_message_mutation(state, request_id, result);
        }
        Message::BackendEvent(BackendEvent::MessageContentChanged {
            address,
            text,
            text_runs,
            is_text,
        }) => {
            for chat in state
                .chat_entries_mut(address.chat_id)
                .filter(|chat| chat.last_message_id == Some(address.message_id))
            {
                chat.last_message_preview =
                    MessagePreview::message_snippet(&text, &text_runs).into();
            }
            if !state.history_pins.is_empty() {
                apply_message_content(
                    &mut state.history_pins,
                    state.open_chat_id,
                    address,
                    text.clone(),
                    text_runs.clone(),
                    is_text,
                );
            }
            apply_message_content(
                &mut state.messages,
                state.open_chat_id,
                address,
                text,
                text_runs,
                is_text,
            );
        }
        Message::BackendEvent(BackendEvent::MessageEdited(address)) => {
            if state.open_chat_id == Some(address.chat_id)
                && let Some(message) = state
                    .messages
                    .iter_mut()
                    .chain(&mut state.history_pins)
                    .find(|message| message.id == address.message_id)
            {
                message.is_edited = true;
                message.spoilers_revealed = false;
            }
        }
        Message::BackendEvent(BackendEvent::MessagesDeleted {
            chat_id,
            mut message_ids,
        }) => {
            message_ids.sort_unstable();
            let deleted = |id: i64| message_ids.binary_search(&id).is_ok();
            apply_message_deletion(&mut state.messages, state.open_chat_id, chat_id, deleted);
            apply_message_deletion(
                &mut state.history_pins,
                state.open_chat_id,
                chat_id,
                deleted,
            );
            if state.open_chat_id == Some(chat_id) {
                if state
                    .message_menu
                    .is_some_and(|menu| deleted(menu.message_id))
                {
                    state.message_menu = None;
                }
                if state.reply_to.is_some_and(deleted) {
                    state.reply_to = None;
                }
                if state
                    .edit
                    .as_ref()
                    .is_some_and(|edit| deleted(edit.message_id))
                {
                    restore_composer(state);
                    state.message_action_error = None;
                }
                if state
                    .delete_confirmation
                    .as_ref()
                    .is_some_and(|confirmation| deleted(confirmation.message_id))
                {
                    state.delete_confirmation = None;
                    state.message_action_error = None;
                }
                if state
                    .forward_selection
                    .as_ref()
                    .is_some_and(|selection| deleted(selection.message_id))
                {
                    state.forward_selection = None;
                    state.forward_chat_summary = None;
                }
            }
        }
        Message::BackendEvent(BackendEvent::ReplyPreviewsLoaded { chat_id, previews }) => {
            state.reply_lookup_pending = false;
            if state.open_chat_id == Some(chat_id) {
                apply_reply_previews(&mut state.messages, previews);
            }
            return refresh_reply_previews(state);
        }
        Message::BackendEvent(BackendEvent::ForwardFinished {
            result,
            discarded_message,
        }) => {
            if let Some((chat_id, message_id)) = discarded_message
                && state.open_chat_id == Some(chat_id)
            {
                state.messages.retain(|message| message.id != message_id);
            }
            state.forward_pending = false;
            state.forward_status = Some(match result {
                Ok(()) => {
                    state.forward_selection = None;
                    state.forward_chat_summary = None;
                    "Message forwarded.".into()
                }
                Err(error) => error,
            });
        }
        Message::Initialized(Ok(())) => {}
        Message::Initialized(Err(error)) => {
            tracing::error!(%error, "Telegram backend initialization command failed");
        }
        Message::PhoneChanged(value) => {
            state.phone = value;
            state.authorization_error = None;
        }
        Message::CodeChanged(value) => {
            state.code = value;
            state.authorization_error = None;
        }
        Message::PasswordChanged(value) => {
            state.password = value;
            state.authorization_error = None;
        }
        Message::SubmitPhone => {
            state.authorization_error = None;
            let backend = state.backend.clone();
            let phone = state.phone.trim().to_owned();
            return Task::perform(
                async move { backend.submit_phone(phone).await },
                Message::AuthorizationCommandSent,
            );
        }
        Message::SubmitCode => {
            state.authorization_error = None;
            let backend = state.backend.clone();
            let code = state.code.trim().to_owned();
            return Task::perform(
                async move { backend.submit_code(code).await },
                Message::AuthorizationCommandSent,
            );
        }
        Message::SubmitPassword => {
            state.authorization_error = None;
            let backend = state.backend.clone();
            let password = state.password.clone();
            state.password.clear();
            return Task::perform(
                async move { backend.submit_password(password).await },
                Message::AuthorizationCommandSent,
            );
        }
        Message::LogOut => {
            state.message_menu = None;
            if state.authorization_state == Some(AuthorizationState::Ready) && !state.logout_pending
            {
                state.logout_pending = true;
                state.authorization_error = None;
                let backend = state.backend.clone();
                return Task::perform(
                    async move { backend.log_out().await },
                    Message::LogoutCommandSent,
                );
            }
        }
        Message::LogoutCommandSent(Err(error)) => {
            state.logout_pending = false;
            state.authorization_error = Some(error.to_string());
        }
        Message::LogoutCommandSent(Ok(())) => {}
        Message::ChatsScrolled(viewport) => {
            state.chat_pages.viewport = Some(viewport);
            if state.chat_pages.error.is_none() {
                if viewport.near_start && state.chat_pages.first_offset > 0 {
                    return request_previous_chat_page(state);
                }
                if viewport.near_end {
                    return request_next_chat_page(state);
                }
            }
        }
        Message::RetryChatPage => {
            return match state.chat_pages.retry {
                Some(ChatPageKind::Previous) => request_previous_chat_page(state),
                Some(ChatPageKind::Refresh) => refresh_chat_pages(state),
                _ => request_next_chat_page(state),
            };
        }
        Message::ChatPageQueued { request_id, result } => {
            if let Err(error) = result {
                fail_chat_page(state, request_id, error.to_string());
            }
        }
        Message::OpenChat(chat_id) => {
            state.message_menu = None;
            if state.open_chat_id != Some(chat_id)
                || (state.history_error.is_some() && state.messages.is_empty())
            {
                state.open_chat_summary = state.chat(chat_id).cloned();
                state.open_chat_id = Some(chat_id);
                state.history_read = HistoryRead::default();
                state.edit = None;
                state.delete_confirmation = None;
                state.message_action_error = None;
                state.reply_to = None;
                state.forward_selection = None;
                state.forward_chat_summary = None;
                state.forward_status = None;
                state.messages.clear();
                state.history_pins.clear();
                state.pending_sends.clear();
                state.has_older_messages = false;
                state.has_newer_messages = false;
                state.history_request = None;
                state.history_failed = None;
                state.history_viewport = None;
                state.composer = text_editor::Content::new();
                state.send_error = None;
                state.history_loading = true;
                state.history_error = None;
                let backend = state.backend.clone();
                return Task::perform(
                    async move { backend.open_chat(chat_id).await },
                    Message::ChatOpenCommandSent,
                );
            }
        }
        Message::ChatOpenCommandSent(Err(error)) => {
            state.history_loading = false;
            state.history_error = Some(error.to_string());
        }
        Message::ChatOpenCommandSent(Ok(())) => {}
        Message::WindowFocusChanged(focused) => {
            state.window_focused = focused;
            if focused {
                state.history_read.failed_through = 0;
            } else {
                state.history_read.viewed_through = 0;
                state.message_menu = None;
            }
        }
        Message::HistoryReadBoundary(address) => {
            if let Some(chat) = state.chat(address.chat_id)
                && state
                    .read_after(chat)
                    .is_some_and(|after| address.message_id > after)
                && state.messages.iter().any(|message| {
                    message.id == address.message_id
                        && message.can_reference
                        && !message.is_outgoing
                })
            {
                state.history_read.viewed_through =
                    state.history_read.viewed_through.max(address.message_id);
                return request_read_receipt(state);
            }
        }
        Message::ReadReceiptQueued { request_id, result } => {
            if result.is_err() {
                return finish_read_receipt(state, request_id, false);
            }
        }
        Message::HistoryScrolled(viewport) => {
            if state.open_chat_id == Some(viewport.chat_id) {
                state.history_viewport = Some(viewport);
                if viewport.direction.is_some() {
                    state.message_menu = None;
                }
                if viewport.near(HistoryEdge::Older) && state.has_older_messages {
                    return request_history_page(state, viewport, HistoryEdge::Older);
                }
                if viewport.near(HistoryEdge::Newer) && state.has_newer_messages {
                    return request_history_page(state, viewport, HistoryEdge::Newer);
                }
            }
        }
        Message::HistoryPageQueued { request_id, result } => {
            if let Err(error) = result {
                fail_history_page(state, request_id, error.to_string());
            }
        }
        Message::ComposerAction(action) => {
            if state.delete_confirmation.is_none()
                && !(state.edit.is_some() && state.mutation_pending())
            {
                state.composer.perform(action);
                state.send_error = None;
                if state.edit.is_some() {
                    state.message_action_error = None;
                }
            }
        }
        Message::OpenMessageMenu {
            chat_id,
            message_id,
            position,
        } => {
            if state.open_chat_id == Some(chat_id)
                && state.delete_confirmation.is_none()
                && let Some(message) = state.referenceable_message(message_id)
            {
                let request_id =
                    (message.is_outgoing && state.edit.is_none() && !state.mutation_pending())
                        .then(|| state.action_request_id());
                if request_id.is_some() {
                    state.message_action_error = None;
                }
                state.message_menu = Some(MessageMenu {
                    message_id,
                    position,
                    actions: MessageActions::default(),
                    request_id,
                });
                if let Some(request_id) = request_id {
                    return request_message_action(
                        state,
                        request_id,
                        MessageAddress {
                            chat_id,
                            message_id,
                        },
                        MessageAction::Inspect,
                    );
                }
            }
        }
        Message::RevealSpoilers(address) => {
            if state.open_chat_id == Some(address.chat_id)
                && let Some(message) = state
                    .messages
                    .iter_mut()
                    .find(|message| message.id == address.message_id)
                && message
                    .text_runs
                    .iter()
                    .any(|run| run.style.contains(TextStyle::SPOILER))
            {
                message.spoilers_revealed = true;
            }
        }
        Message::DismissMessageMenu => state.message_menu = None,
        Message::Escape => {
            if state.message_menu.take().is_none()
                && (state.delete_confirmation.take().is_some() || !state.mutation_pending())
            {
                restore_composer(state);
                state.message_action_error = None;
            }
        }
        Message::StartEdit(message_id) => {
            let actions = take_menu_actions(state, message_id);
            if actions.can_edit
                && !state.mutation_pending()
                && state.edit.is_none()
                && let Some(message) = state.referenceable_message(message_id)
                && message.is_outgoing
                && message.is_text
            {
                let edit_content = text_editor::Content::with_text(&message.text);
                state.edit = Some(EditSession {
                    message_id,
                    draft: std::mem::replace(&mut state.composer, edit_content),
                });
                state.message_action_error = None;
                return iced::widget::operation::focus(CHAT_COMPOSER_ID);
            }
        }
        Message::CancelEdit => {
            if !state.mutation_pending() && state.edit.is_some() {
                restore_composer(state);
                state.message_action_error = None;
                return iced::widget::operation::focus(CHAT_COMPOSER_ID);
            }
        }
        Message::ShowDeleteConfirmation(message_id) => {
            let actions = take_menu_actions(state, message_id);
            if (actions.can_delete_for_self || actions.can_delete_for_everyone)
                && state.edit.is_none()
                && !state.mutation_pending()
                && state
                    .referenceable_message(message_id)
                    .is_some_and(|message| message.is_outgoing)
            {
                state.delete_confirmation = Some(DeleteConfirmation {
                    message_id,
                    actions,
                });
                state.message_action_error = None;
            }
        }
        Message::ConfirmDelete(scope) => {
            if !state.mutation_pending()
                && let Some(chat_id) = state.open_chat_id
                && let Some(confirmation) = &state.delete_confirmation
                && confirmation.permits(scope)
            {
                let address = MessageAddress {
                    chat_id,
                    message_id: confirmation.message_id,
                };
                return start_message_mutation(state, address, MessageAction::Delete(scope));
            }
        }
        Message::CancelDelete => {
            state.delete_confirmation = None;
            state.message_action_error = None;
        }
        Message::ActionQueued { request_id, result } => {
            if let Err(error) = result {
                if state
                    .pending_mutation
                    .as_ref()
                    .is_some_and(|pending| pending.request_id == request_id)
                {
                    finish_message_mutation(state, request_id, Err(error.to_string()));
                } else {
                    apply_message_actions(state, request_id, Err(error.to_string()));
                }
            }
        }
        Message::SelectReply(message_id) => {
            state.message_menu = None;
            if state.edit.is_none() && state.referenceable_message(message_id).is_some() {
                state.reply_to = Some(message_id);
                return iced::widget::operation::focus(CHAT_COMPOSER_ID);
            }
        }
        Message::CancelReply => state.reply_to = None,
        Message::ReplyLookupQueued(result) => {
            if result.is_err() {
                state.reply_lookup_pending = false;
                for message in &mut state.messages {
                    if let Some(reply) = message
                        .reply
                        .as_mut()
                        .filter(|reply| reply.preview.is_none())
                    {
                        reply.resolve(None);
                    }
                }
            }
        }
        Message::StartForward(message_id) => {
            state.message_menu = None;
            if !state.forward_pending && state.referenceable_message(message_id).is_some() {
                state.forward_status = None;
                state.forward_selection = Some(ForwardSelection {
                    message_id,
                    destination: None,
                });
            }
        }
        Message::SelectForwardDestination(chat_id) => {
            if state.can_forward_to(chat_id) && state.forward_selection.is_some() {
                state.forward_chat_summary = state.chat(chat_id).cloned();
                if let Some(selection) = &mut state.forward_selection {
                    selection.destination = Some(chat_id);
                }
                state.forward_status = None;
            }
        }
        Message::ConfirmForward => {
            let Some(from_chat_id) = state.open_chat_id else {
                return Task::none();
            };
            let Some(selection) = &state.forward_selection else {
                return Task::none();
            };
            let Some(to_chat_id) = selection.destination.filter(|id| state.can_forward_to(*id))
            else {
                return Task::none();
            };
            let message_id = selection.message_id;
            if state.referenceable_message(message_id).is_some() {
                state.forward_pending = true;
                state.forward_status = None;
                let backend = state.backend.clone();
                return Task::perform(
                    async move { backend.forward(from_chat_id, message_id, to_chat_id).await },
                    Message::ForwardQueued,
                );
            }
        }
        Message::CancelForward => {
            state.forward_selection = None;
            state.forward_chat_summary = None;
            state.forward_status = None;
        }
        Message::ForwardQueued(result) => {
            if let Err(error) = result {
                state.forward_pending = false;
                state.forward_status = Some(error.to_string());
            }
        }
        Message::Submit => {
            if state.delete_confirmation.is_some() {
                return Task::none();
            }
            let Some(chat_id) = state.open_chat_id else {
                return Task::none();
            };
            let text = state.composer.text();
            if text.trim().is_empty() {
                return Task::none();
            }
            if let Some(edit) = &state.edit {
                if state.mutation_pending() {
                    return Task::none();
                }
                let Some(original) = state.referenceable_message(edit.message_id) else {
                    restore_composer(state);
                    return Task::none();
                };
                if text == original.text {
                    restore_composer(state);
                    state.message_action_error = None;
                    return Task::none();
                }
                let address = MessageAddress {
                    chat_id,
                    message_id: edit.message_id,
                };
                return start_message_mutation(state, address, MessageAction::Edit(text));
            }
            if state.pending_sends.len() >= MAX_PENDING_SENDS {
                state.send_error = Some("Wait for earlier messages to finish sending.".into());
                return Task::none();
            }
            let reply = state.reply_message().map(|original| {
                Box::new(MessageReply {
                    chat_id,
                    message_id: original.id,
                    quote: None,
                    preview: Some(MessagePreview::from_text(
                        &original.sender_name,
                        &original.text,
                        &original.text_runs,
                    )),
                })
            });
            let reply_to = reply.as_ref().map(|reply| reply.message_id);
            state.reply_to = None;
            let message_id = state.next_temporary_message_id;
            state.next_temporary_message_id = state.next_temporary_message_id.saturating_sub(1);
            state.pending_sends.insert(message_id, reply_to);
            let reload_history = state.has_newer_messages;
            if reload_history {
                let selected = history_pin_ids(state);
                state.history_pins.retain(|pin| selected.contains(&pin.id));
                for message in std::mem::take(&mut state.messages) {
                    if state.pending_sends.contains_key(&message.id) {
                        state.messages.push(message);
                    } else if selected.contains(&message.id) {
                        state.history_pins.push(message);
                    }
                }
                state.history_loading = true;
                state.history_error = None;
                state.has_older_messages = false;
                state.has_newer_messages = false;
                state.history_request = None;
                state.history_failed = None;
            }
            state.history_viewport = None;
            insert_live_message(
                state,
                ChatMessage {
                    id: message_id,
                    date: current_unix_timestamp(),
                    sender_name: "You".into(),
                    text: text.clone(),
                    text_runs: Box::default(),
                    is_outgoing: true,
                    can_reference: false,
                    is_text: true,
                    is_edited: false,
                    spoilers_revealed: false,
                    reply,
                    forwarded_from: None,
                },
            );
            state.history_end_revision = state.history_end_revision.wrapping_add(1);
            state.composer = text_editor::Content::new();
            state.send_error = None;
            let backend = state.backend.clone();
            return Task::perform(
                async move {
                    let result = async {
                        backend
                            .send_text(chat_id, message_id, text.clone(), reply_to)
                            .await?;
                        if reload_history {
                            backend.open_chat(chat_id).await?;
                        }
                        Ok(())
                    }
                    .await;
                    (chat_id, message_id, text, result)
                },
                |(chat_id, message_id, text, result)| Message::SendMessageCommandSent {
                    chat_id,
                    message_id,
                    text,
                    result,
                },
            );
        }
        Message::SendMessageCommandSent {
            chat_id,
            message_id,
            text,
            result,
        } => {
            if let Err(error) = result
                && state.open_chat_id == Some(chat_id)
            {
                state.history_loading = false;
                restore_rejected_message(state, message_id, &text);
                state.send_error = Some(error.to_string());
            }
        }
        Message::AuthorizationCommandSent(Err(error)) => {
            state.authorization_error = Some(error.to_string());
        }
        Message::AuthorizationCommandSent(Ok(())) => {}
    }
    Task::none()
}

fn sort_chats(chats: &mut [ChatSummary]) {
    chats.sort_unstable_by_key(|chat| std::cmp::Reverse((chat.position, chat.id)));
}

fn add_live_chat(state: &mut State, chat: ChatSummary) {
    let limit = state
        .chat_pages
        .next_offset
        .saturating_sub(state.chat_pages.first_offset);
    if !state.chat_pages.finished
        && !state
            .chats
            .iter()
            .take(limit)
            .next_back()
            .is_some_and(|last| (chat.position, chat.id) > (last.position, last.id))
    {
        return;
    }
    if state.chat_pages.first_offset > 0
        && state
            .chats
            .first()
            .is_some_and(|first| (chat.position, chat.id) > (first.position, first.id))
    {
        state.chat_pages.first_offset += 1;
        state.chat_pages.next_offset += 1;
        return;
    }
    state.chats.push(chat);
    sort_chats(&mut state.chats);
    if state.chat_pages.finished {
        state.chat_pages.next_offset += 1;
        trim_chats(state, ChatPageKind::Previous);
    } else {
        state.chats.truncate(limit.min(CHAT_CACHE_LIMIT));
    }
}

fn update_chat_summary(existing: &mut ChatSummary, mut incoming: ChatSummary) {
    // A snapshot can overtake a queued inbox or outbox update.
    if incoming.last_read_inbox_message_id < existing.last_read_inbox_message_id {
        incoming.last_read_inbox_message_id = existing.last_read_inbox_message_id;
        incoming.unread_count = existing.unread_count;
    }
    if let (Some(read), Some(previous)) = (
        &mut incoming.last_read_outbox_message_id,
        existing.last_read_outbox_message_id,
    ) {
        *read = (*read).max(previous);
    }
    *existing = incoming;
}

fn update_pinned_chat(state: &mut State, incoming: &ChatSummary) {
    for selected in state
        .open_chat_summary
        .iter_mut()
        .chain(&mut state.forward_chat_summary)
    {
        if selected.id == incoming.id {
            update_chat_summary(selected, incoming.clone());
        }
    }
}

fn trim_chats(state: &mut State, arrival: ChatPageKind) {
    let excess = state.chats.len().saturating_sub(CHAT_CACHE_LIMIT);
    if excess == 0 {
        return;
    }
    if arrival == ChatPageKind::Next {
        state.chats.drain(..excess);
        state.chat_pages.first_offset += excess;
    } else {
        state.chats.truncate(CHAT_CACHE_LIMIT);
        state.chat_pages.next_offset = state.chat_pages.next_offset.saturating_sub(excess);
        state.chat_pages.after = state.chats.last().map(|chat| chat.id);
        state.chat_pages.finished = false;
    }
}

fn restore_chat_scroll(state: &State, viewport: Option<ChatListViewport>) -> Task<Message> {
    let Some(viewport) = viewport else {
        return Task::none();
    };
    let Some(index) = state
        .chats
        .iter()
        .position(|chat| Some(chat.id) == viewport.anchor)
    else {
        return Task::none();
    };
    iced::widget::operation::scroll_to(
        CHAT_LIST_SCROLL_ID,
        iced::widget::operation::AbsoluteOffset {
            x: 0.0,
            y: index as f32 * viewport.row_height + viewport.inset,
        },
    )
}

fn request_next_chat_page(state: &mut State) -> Task<Message> {
    if state.chat_pages.finished {
        return Task::none();
    }
    request_chat_page(
        state,
        state.chat_pages.next_offset,
        CHAT_PAGE_SIZE,
        state
            .chat_pages
            .after
            .or_else(|| state.chats.last().map(|chat| chat.id))
            .map(ChatPageAnchor::After),
        ChatPageKind::Next,
    )
}

fn request_previous_chat_page(state: &mut State) -> Task<Message> {
    let offset = state.chat_pages.first_offset;
    if offset == 0 {
        return Task::none();
    }
    request_chat_page(
        state,
        offset.saturating_sub(CHAT_PAGE_SIZE),
        offset.min(CHAT_PAGE_SIZE),
        state
            .chats
            .first()
            .map(|chat| ChatPageAnchor::Before(chat.id)),
        ChatPageKind::Previous,
    )
}

fn refresh_pending_chat_pages(state: &mut State) -> Task<Message> {
    if state.chat_pages.refresh_required {
        refresh_chat_pages(state)
    } else {
        Task::none()
    }
}

fn refresh_chat_pages(state: &mut State) -> Task<Message> {
    if state.chat_pages.pending.is_some() {
        return Task::none();
    }
    let offset = state.chat_pages.first_offset;
    let count = state
        .chat_pages
        .next_offset
        .saturating_sub(offset)
        .min(CHAT_CACHE_LIMIT);
    state.chat_pages.refresh_required = false;
    if count == 0 {
        return Task::none();
    }
    request_chat_page(state, offset, count, None, ChatPageKind::Refresh)
}

fn request_chat_page(
    state: &mut State,
    offset: usize,
    limit: usize,
    anchor: Option<ChatPageAnchor>,
    kind: ChatPageKind,
) -> Task<Message> {
    if state.chat_pages.pending.is_some()
        || state.logout_pending
        || state.authorization_state != Some(AuthorizationState::Ready)
        || state.logout_pending
    {
        return Task::none();
    }
    let request_id = state.action_request_id();
    let query = ChatPageQuery {
        request_id,
        offset,
        limit,
        anchor,
        load_missing: kind == ChatPageKind::Next,
    };
    state.chat_pages.started = true;
    state.chat_pages.pending = Some((query, kind));
    state.chat_pages.error = None;
    let backend = state.backend.clone();
    Task::perform(
        async move { backend.load_chat_page(query).await },
        move |result| Message::ChatPageQueued { request_id, result },
    )
}

fn fail_chat_page(state: &mut State, request_id: u64, message: String) {
    if let Some((_, kind)) = state
        .chat_pages
        .pending
        .filter(|(query, _)| query.request_id == request_id)
    {
        state.chat_pages.pending = None;
        state.chat_pages.retry = Some(kind);
        state.chat_pages.error = Some(message);
    }
}

fn request_history_page(
    state: &mut State,
    viewport: HistoryViewport,
    edge: HistoryEdge,
) -> Task<Message> {
    if state.history_request.is_some()
        || state.history_loading
        || state.logout_pending
        || (state.history_failed == Some(edge)
            && viewport.direction
                != Some(match edge {
                    HistoryEdge::Older => HistoryScrollDirection::Up,
                    HistoryEdge::Newer => HistoryScrollDirection::Down,
                }))
    {
        return Task::none();
    }
    let ids = state
        .messages
        .iter()
        .filter(|message| message.can_reference && message.id > 0)
        .map(|message| message.id);
    let boundary = match edge {
        HistoryEdge::Older => ids.min(),
        HistoryEdge::Newer => ids.max(),
    };
    let Some(boundary) = boundary else {
        return Task::none();
    };
    let request_id = state.action_request_id();
    state.history_request = Some((request_id, edge));
    state.history_failed = None;
    state.history_error = None;
    let backend = state.backend.clone();
    Task::perform(
        async move {
            backend
                .load_history_page(viewport.chat_id, boundary, edge, request_id)
                .await
        },
        move |result| Message::HistoryPageQueued { request_id, result },
    )
}

fn fail_history_page(state: &mut State, request_id: u64, message: String) {
    if let Some((_, edge)) = state.history_request.filter(|(id, _)| *id == request_id) {
        state.history_request = None;
        state.history_failed = Some(edge);
        state.history_error = Some(message);
    }
}

fn request_read_receipt(state: &mut State) -> Task<Message> {
    let Some(chat) = state.open_chat_id.and_then(|id| state.chat(id)) else {
        return Task::none();
    };
    let message_id = state.history_read.viewed_through;
    if state.read_in_flight.is_some()
        || !state.can_mark_read(chat)
        || message_id <= state.read_threshold(chat)
    {
        return Task::none();
    }

    let address = MessageAddress {
        chat_id: chat.id,
        message_id,
    };
    let request_id = state.action_request_id();
    state.read_in_flight = Some(PendingRead {
        request_id,
        address,
    });
    let backend = state.backend.clone();
    Task::perform(
        async move { backend.mark_read(address, request_id).await },
        move |result| Message::ReadReceiptQueued { request_id, result },
    )
}

fn finish_read_receipt(state: &mut State, request_id: u64, accepted: bool) -> Task<Message> {
    if !state
        .read_in_flight
        .as_ref()
        .is_some_and(|pending| pending.request_id == request_id)
    {
        return Task::none();
    }
    if let Some(pending) = state.read_in_flight.take()
        && state.open_chat_id == Some(pending.address.chat_id)
    {
        let threshold = if accepted {
            &mut state.history_read.accepted_through
        } else {
            &mut state.history_read.failed_through
        };
        *threshold = (*threshold).max(pending.address.message_id);
    }
    request_read_receipt(state)
}

fn request_message_action(
    state: &State,
    request_id: u64,
    address: MessageAddress,
    action: MessageAction,
) -> Task<Message> {
    let backend = state.backend.clone();
    Task::perform(
        async move { backend.message_action(address, request_id, action).await },
        move |result| Message::ActionQueued { request_id, result },
    )
}

fn start_message_mutation(
    state: &mut State,
    address: MessageAddress,
    action: MessageAction,
) -> Task<Message> {
    let request_id = state.action_request_id();
    state.pending_mutation = Some(PendingMutation {
        request_id,
        address,
    });
    state.message_action_error = None;
    request_message_action(state, request_id, address, action)
}

fn finish_message_mutation(state: &mut State, request_id: u64, result: Result<(), String>) {
    let Some(pending) = state
        .pending_mutation
        .as_ref()
        .filter(|pending| pending.request_id == request_id)
    else {
        return;
    };
    let address = pending.address;
    state.pending_mutation = None;
    if state.open_chat_id != Some(address.chat_id)
        || state.referenceable_message(address.message_id).is_none()
    {
        return;
    }
    match result {
        Ok(()) => {
            state.delete_confirmation = None;
            restore_composer(state);
            state.message_action_error = None;
        }
        Err(error) => state.message_action_error = Some(error),
    }
}

fn restore_composer(state: &mut State) {
    if let Some(edit) = state.edit.take() {
        state.composer = edit.draft;
    }
}

fn apply_message_actions(
    state: &mut State,
    request_id: u64,
    result: Result<MessageActions, String>,
) {
    let Some(menu) = state
        .message_menu
        .filter(|menu| menu.request_id == Some(request_id))
    else {
        return;
    };
    let mut actions = match result {
        Ok(actions) => actions,
        Err(error) => {
            state.message_action_error = Some(error);
            MessageActions::default()
        }
    };
    actions.can_edit &= state
        .referenceable_message(menu.message_id)
        .is_some_and(|message| message.is_text);
    state.message_menu = Some(MessageMenu {
        actions,
        request_id: None,
        ..menu
    });
}

fn take_menu_actions(state: &mut State, message_id: i64) -> MessageActions {
    state
        .message_menu
        .take()
        .filter(|menu| menu.message_id == message_id)
        .map_or_else(MessageActions::default, |menu| menu.actions)
}

fn apply_message_content(
    messages: &mut [ChatMessage],
    open_chat_id: Option<i64>,
    address: MessageAddress,
    text: String,
    text_runs: Box<[TextRun]>,
    is_text: bool,
) {
    let original_index = (open_chat_id == Some(address.chat_id))
        .then(|| {
            messages
                .iter()
                .position(|message| message.id == address.message_id)
        })
        .flatten();
    let snippet = messages
        .iter()
        .any(|message| {
            message.reply.as_ref().is_some_and(|reply| {
                reply.chat_id == address.chat_id
                    && reply.message_id == address.message_id
                    && reply.quote.is_none()
            })
        })
        .then(|| MessagePreview::message_snippet(&text, &text_runs));

    if let Some(snippet) = snippet {
        let sender =
            original_index.map(|index| MessagePreview::snippet(&messages[index].sender_name));
        for reply in messages
            .iter_mut()
            .filter_map(|message| message.reply.as_mut())
            .filter(|reply| {
                reply.chat_id == address.chat_id
                    && reply.message_id == address.message_id
                    && reply.quote.is_none()
            })
        {
            let sender_name = reply
                .preview
                .take()
                .and_then(|preview| preview.sender_name)
                .or_else(|| sender.clone());
            reply.resolve(Some(MessagePreview {
                sender_name,
                text: snippet.clone(),
            }));
        }
    }
    if let Some(index) = original_index {
        messages[index].text = text;
        messages[index].text_runs = text_runs;
        messages[index].spoilers_revealed = false;
        messages[index].is_text = is_text;
    }
}

fn apply_message_deletion(
    messages: &mut Vec<ChatMessage>,
    open_chat_id: Option<i64>,
    chat_id: i64,
    deleted: impl Fn(i64) -> bool,
) {
    if open_chat_id == Some(chat_id) {
        messages.retain(|message| !deleted(message.id));
    }
    for reply in messages
        .iter_mut()
        .filter_map(|message| message.reply.as_mut())
    {
        if reply.chat_id == chat_id && deleted(reply.message_id) {
            reply.resolve(None);
        }
    }
}

impl DeleteConfirmation {
    fn permits(&self, scope: DeleteScope) -> bool {
        match scope {
            DeleteScope::ForSelf => self.actions.can_delete_for_self,
            DeleteScope::ForEveryone => self.actions.can_delete_for_everyone,
        }
    }
}

fn refresh_reply_previews(state: &mut State) -> Task<Message> {
    let Some(chat_id) = state.open_chat_id else {
        return Task::none();
    };
    let message_ids =
        collect_reply_lookups(&mut state.messages, chat_id, !state.reply_lookup_pending);
    if message_ids.is_empty() {
        return Task::none();
    }
    state.reply_lookup_pending = true;
    let backend = state.backend.clone();
    Task::perform(
        async move { backend.resolve_replies(chat_id, message_ids).await },
        Message::ReplyLookupQueued,
    )
}

fn collect_reply_lookups(messages: &mut [ChatMessage], chat_id: i64, enqueue: bool) -> Vec<i64> {
    if !messages.iter().any(|message| {
        message
            .reply
            .as_ref()
            .is_some_and(|reply| reply.preview.is_none())
    }) {
        return Vec::new();
    }
    let indexes: HashMap<_, _> = messages
        .iter()
        .enumerate()
        .map(|(index, message)| (message.id, index))
        .collect();
    let mut lookups = Vec::new();
    for index in (0..messages.len()).rev() {
        let message = &messages[index];
        let Some(reply) = message
            .reply
            .as_ref()
            .filter(|reply| reply.preview.is_none())
        else {
            continue;
        };
        let original = (reply.chat_id == chat_id)
            .then(|| indexes.get(&reply.message_id))
            .flatten()
            .map(|&index| &messages[index]);
        if let Some(original) = original {
            let preview = MessagePreview::from_text(
                &original.sender_name,
                &original.text,
                &original.text_runs,
            );
            if let Some(reply) = &mut messages[index].reply {
                reply.resolve(Some(preview));
            }
        } else if enqueue && message.can_reference && lookups.len() < REPLY_LOOKUP_BATCH_SIZE {
            lookups.push(message.id);
        }
    }
    lookups
}

fn apply_reply_previews(
    messages: &mut [ChatMessage],
    previews: impl IntoIterator<Item = (i64, Option<MessagePreview>)>,
) {
    for (message_id, preview) in previews {
        if let Some(reply) = messages
            .iter_mut()
            .find(|message| message.id == message_id)
            .and_then(|message| message.reply.as_mut())
            .filter(|reply| reply.preview.is_none())
        {
            reply.resolve(preview);
        }
    }
}

fn restore_rejected_message(state: &mut State, message_id: i64, text: &str) {
    let reply_to = state.pending_sends.remove(&message_id).flatten();
    if state.edit.is_none() && state.composer.text().is_empty() && state.reply_to.is_none() {
        state.reply_to = reply_to;
        state.composer = text_editor::Content::with_text(text);
    }
    state.messages.retain(|message| message.id != message_id);
}

fn insert_message(messages: &mut Vec<ChatMessage>, message: ChatMessage) {
    if let Some(existing) = messages
        .iter_mut()
        .find(|existing| existing.id == message.id)
    {
        *existing = message;
    } else {
        messages.push(message);
    }
    messages.sort_by_key(|message| (message.date, message.id));
}

fn merge_history_page(existing: &mut Vec<ChatMessage>, older: Vec<ChatMessage>) {
    let mut known_ids: HashSet<_> = existing.iter().map(|message| message.id).collect();
    let mut combined = Vec::with_capacity(existing.len() + older.len());
    combined.extend(
        older
            .into_iter()
            .filter(|message| known_ids.insert(message.id)),
    );
    combined.append(existing);
    combined.sort_by_key(|message| (message.date, message.id));
    *existing = combined;
}

fn history_pin_ids(state: &State) -> Vec<i64> {
    [
        state.reply_to,
        state.edit.as_ref().map(|edit| edit.message_id),
        state
            .forward_selection
            .as_ref()
            .map(|selection| selection.message_id),
        state
            .delete_confirmation
            .as_ref()
            .map(|confirmation| confirmation.message_id),
    ]
    .into_iter()
    .chain(state.pending_sends.values().copied())
    .flatten()
    .collect()
}

fn trim_history(state: &mut State, arrival: HistoryEdge) {
    let excess = state.messages.len().saturating_sub(HISTORY_CACHE_LIMIT);
    if excess == 0 {
        return;
    }
    let selected = history_pin_ids(state);
    let keep = |message: &ChatMessage| selected.contains(&message.id);
    state.history_pins.retain(keep);
    let evicted = match arrival {
        HistoryEdge::Older => {
            state.has_newer_messages = true;
            state.messages.drain(HISTORY_CACHE_LIMIT..)
        }
        HistoryEdge::Newer => {
            state.has_older_messages = true;
            state.messages.drain(..excess)
        }
    };
    state.history_pins.extend(evicted.filter(keep));
}

fn insert_live_message(state: &mut State, message: ChatMessage) {
    if state
        .messages
        .iter()
        .any(|existing| existing.id == message.id)
    {
        insert_message(&mut state.messages, message);
    } else if !state.has_newer_messages {
        if state.messages.len() >= HISTORY_CACHE_LIMIT
            && state
                .history_viewport
                .is_some_and(|viewport| viewport.near(HistoryEdge::Older))
        {
            state.has_newer_messages = true;
        } else {
            insert_message(&mut state.messages, message);
            trim_history(state, HistoryEdge::Newer);
        }
    }
}

fn current_unix_timestamp() -> i32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i32)
}

impl State {
    pub(crate) fn color_scheme(&self) -> crate::config::ColorScheme {
        self.color_scheme
    }
    pub(crate) fn authorization_state(&self) -> Option<AuthorizationState> {
        self.authorization_state
    }
    pub(crate) fn phone(&self) -> &str {
        &self.phone
    }
    pub(crate) fn code(&self) -> &str {
        &self.code
    }
    pub(crate) fn password(&self) -> &str {
        &self.password
    }
    pub(crate) fn authorization_error(&self) -> Option<&str> {
        self.authorization_error.as_deref()
    }
    pub(crate) fn logout_pending(&self) -> bool {
        self.logout_pending
    }
    pub(crate) fn chats(&self) -> &[ChatSummary] {
        &self.chats
    }
    pub(crate) fn chat(&self, id: i64) -> Option<&ChatSummary> {
        self.chats
            .iter()
            .chain(&self.open_chat_summary)
            .chain(&self.forward_chat_summary)
            .find(|chat| chat.id == id)
    }

    fn chat_entries_mut(&mut self, id: i64) -> impl Iterator<Item = &mut ChatSummary> {
        self.chats
            .iter_mut()
            .chain(&mut self.open_chat_summary)
            .chain(&mut self.forward_chat_summary)
            .filter(move |chat| chat.id == id)
    }

    pub(crate) fn chat_page_loading(&self) -> bool {
        self.chat_pages.pending.is_some()
    }

    pub(crate) fn chat_page_error(&self) -> Option<&str> {
        self.chat_pages.error.as_deref()
    }

    fn can_mark_read(&self, chat: &ChatSummary) -> bool {
        self.authorization_state == Some(AuthorizationState::Ready)
            && self.window_focused
            && self.open_chat_id == Some(chat.id)
            && !self.logout_pending
            && !self.history_loading
            && self.delete_confirmation.is_none()
            && chat.unread_count > 0
    }

    fn read_threshold(&self, chat: &ChatSummary) -> i64 {
        chat.last_read_inbox_message_id
            .max(self.history_read.accepted_through)
            .max(self.history_read.failed_through)
    }

    pub(crate) fn read_after(&self, chat: &ChatSummary) -> Option<i64> {
        self.can_mark_read(chat).then(|| {
            let mut after = self.read_threshold(chat);
            if let Some(pending) = &self.read_in_flight {
                if pending.address.chat_id == chat.id {
                    after = after.max(pending.address.message_id);
                }
                after = after.max(self.history_read.viewed_through);
            }
            after
        })
    }

    pub(crate) fn open_chat_id(&self) -> Option<i64> {
        self.open_chat_id
    }
    pub(crate) fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }
    pub(crate) fn history_loading(&self) -> bool {
        self.history_loading
    }
    pub(crate) fn history_page_edge(&self) -> Option<HistoryEdge> {
        self.history_request
            .map(|(_, edge)| edge)
            .or(self.history_failed)
    }

    pub(crate) fn has_newer_history(&self) -> bool {
        self.has_newer_messages
    }
    pub(crate) fn history_error(&self) -> Option<&str> {
        self.history_error.as_deref()
    }
    pub(crate) fn history_end_revision(&self) -> u64 {
        self.history_end_revision
    }
    pub(crate) fn referenceable_message(&self, message_id: i64) -> Option<&ChatMessage> {
        self.messages
            .iter()
            .chain(&self.history_pins)
            .find(|message| message.id == message_id && message.can_reference)
    }
    fn action_request_id(&mut self) -> u64 {
        self.next_action_request_id = self.next_action_request_id.wrapping_add(1);
        self.next_action_request_id
    }
    pub(crate) fn is_editing(&self) -> bool {
        self.edit.is_some()
    }
    pub(crate) fn mutation_pending(&self) -> bool {
        self.pending_mutation.is_some()
    }
    pub(crate) fn delete_confirmation(&self) -> Option<&DeleteConfirmation> {
        self.delete_confirmation.as_ref()
    }
    pub(crate) fn message_action_error(&self) -> Option<&str> {
        self.message_action_error.as_deref()
    }
    pub(crate) fn message_menu(&self) -> Option<MessageMenu> {
        self.message_menu
            .filter(|menu| self.referenceable_message(menu.message_id).is_some())
    }
    pub(crate) fn reply_message(&self) -> Option<&ChatMessage> {
        self.reply_to.and_then(|id| self.referenceable_message(id))
    }
    pub(crate) fn forward_message(&self) -> Option<&ChatMessage> {
        self.forward_selection
            .as_ref()
            .and_then(|selection| self.referenceable_message(selection.message_id))
    }
    pub(crate) fn forward_selection(&self) -> Option<&ForwardSelection> {
        self.forward_selection.as_ref()
    }
    pub(crate) fn can_forward_to(&self, chat_id: i64) -> bool {
        !self.forward_pending
            && self.open_chat_id != Some(chat_id)
            && self
                .chat(chat_id)
                .is_some_and(|chat| chat.can_receive_forwards)
    }
    pub(crate) fn forward_pending(&self) -> bool {
        self.forward_pending
    }
    pub(crate) fn forward_status(&self) -> Option<&str> {
        self.forward_status.as_deref()
    }
    pub(crate) fn composer(&self) -> &text_editor::Content {
        &self.composer
    }
    pub(crate) fn send_error(&self) -> Option<&str> {
        self.send_error.as_deref()
    }
}

#[cfg(test)]
pub(crate) fn visual_history_page(state: &mut State, messages: Vec<ChatMessage>) {
    let request_id = state.action_request_id();
    // Headless fixtures can model longer histories than the runtime cache.
    merge_history_page(&mut state.messages, messages);
    state.history_request = Some((request_id, HistoryEdge::Older));
    drop(update(
        state,
        Message::BackendEvent(BackendEvent::HistoryPageLoaded {
            request_id,
            messages: Vec::new(),
            has_more: true,
        }),
    ));
}

#[cfg(test)]
pub(crate) fn visual_fixture(
    scheme: crate::config::ColorScheme,
    chats: Vec<ChatSummary>,
    messages: Vec<ChatMessage>,
) -> State {
    let (mut state, _) = boot(Adapter::for_tests(), scheme);
    state.authorization_state = Some(AuthorizationState::Ready);
    state.open_chat_id = chats.first().map(|chat| chat.id);
    state.chats = chats;
    state.messages = messages;
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: i64) -> ChatMessage {
        ChatMessage {
            id,
            date: 0,
            sender_name: "Anna".into(),
            text: "first line\nsecond line".into(),
            text_runs: Box::default(),
            is_outgoing: false,
            can_reference: true,
            is_text: true,
            is_edited: false,
            spoilers_revealed: false,
            reply: None,
            forwarded_from: None,
        }
    }

    fn reply(id: i64, original_chat_id: i64, original_message_id: i64) -> ChatMessage {
        let mut message = message(id);
        message.reply = Some(Box::new(MessageReply {
            chat_id: original_chat_id,
            message_id: original_message_id,
            quote: None,
            preview: None,
        }));
        message
    }

    #[test]
    fn history_window_evicts_both_edges_and_pins_an_active_reply() {
        let limit = HISTORY_CACHE_LIMIT as i64;
        let mut state = visual_fixture(
            crate::config::ColorScheme::Light,
            vec![unread_chat(10), unread_chat(11)],
            (51..=50 + limit).map(message).collect(),
        );
        state.reply_to = Some(50 + limit);
        state.has_older_messages = true;
        let viewport = |offset_y, remaining_y| {
            Message::HistoryScrolled(HistoryViewport {
                chat_id: 10,
                offset_y,
                remaining_y,
                height: 500.0,
                direction: Some(HistoryScrollDirection::Up),
            })
        };

        drop(update(&mut state, viewport(10.0, 5000.0)));
        let (request_id, edge) = state.history_request.unwrap();
        assert_eq!(edge, HistoryEdge::Older);
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::HistoryPageLoaded {
                request_id,
                messages: (1..=50).map(message).collect(),
                has_more: false,
            }),
        ));
        assert!(
            state
                .messages
                .iter()
                .map(|message| message.id)
                .eq(1..=limit)
        );
        assert_eq!(state.reply_message().unwrap().id, 50 + limit);
        assert!(state.has_newer_messages);

        insert_live_message(&mut state, message(limit + 51));
        assert_eq!(state.messages.len(), HISTORY_CACHE_LIMIT);
        drop(update(&mut state, viewport(5000.0, 10.0)));
        let (request_id, edge) = state.history_request.unwrap();
        assert_eq!(edge, HistoryEdge::Newer);
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::HistoryPageLoaded {
                request_id,
                messages: (limit + 1..=limit + 50).map(message).collect(),
                has_more: true,
            }),
        ));
        assert!(
            state
                .messages
                .iter()
                .map(|message| message.id)
                .eq(51..=limit + 50)
        );
        assert!(state.has_older_messages);
        assert!(state.history_pins.is_empty());

        drop(update(&mut state, Message::OpenChat(11)));
        assert!(state.messages.is_empty() && state.history_pins.is_empty());
        assert!(!state.has_newer_messages);
    }

    #[test]
    fn sending_from_an_evicted_history_reopens_the_latest_page() {
        let mut state = visual_fixture(
            crate::config::ColorScheme::Light,
            vec![unread_chat(10)],
            (1..=HISTORY_CACHE_LIMIT as i64).map(message).collect(),
        );
        state.has_newer_messages = true;
        state.composer = text_editor::Content::with_text("Sent from older history");
        state.reply_to = Some(100);
        let revision = state.history_end_revision;
        drop(update(&mut state, Message::Submit));
        assert_eq!(state.history_end_revision, revision + 1);
        assert!(!state.has_newer_messages);
        assert_eq!(state.messages.len(), 1);
        assert_eq!(state.messages[0].text, "Sent from older history");
        let pending_id = state.messages[0].id;

        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatHistoryLoaded {
                chat_id: 10,
                messages: (351..=400).map(message).collect(),
                has_older_messages: true,
            }),
        ));
        assert!(
            state.messages[..50]
                .iter()
                .map(|entry| entry.id)
                .eq(351..=400)
        );
        assert_eq!(state.messages.last().unwrap().id, pending_id);
        assert!(!state.history_loading);
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::MessageSendRejected {
                chat_id: 10,
                message_id: pending_id,
                text: "Sent from older history".into(),
                reason: "Send failed".into(),
            }),
        ));
        assert_eq!(state.composer.text(), "Sent from older history");
        assert_eq!(state.reply_message().unwrap().id, 100);
    }

    #[test]
    fn dialog_window_loads_evicted_pages_and_keeps_the_open_chat() {
        let ranked = |id: i64| {
            let mut chat = unread_chat(id);
            chat.position = 1000 - id;
            chat
        };
        let limit = CHAT_CACHE_LIMIT as i64;
        let mut state = visual_fixture(
            crate::config::ColorScheme::Light,
            (1..=limit).map(ranked).collect(),
            Vec::new(),
        );
        state.open_chat_summary = Some(ranked(1));
        state.chat_pages.started = true;
        state.chat_pages.next_offset = CHAT_CACHE_LIMIT;
        let viewport = |anchor, near_start, near_end| {
            Message::ChatsScrolled(ChatListViewport {
                anchor: Some(anchor),
                inset: 25.0,
                row_height: 60.0,
                near_start,
                near_end,
            })
        };
        drop(update(&mut state, viewport(limit - 4, false, true)));
        let (next, _) = state.chat_pages.pending.unwrap();
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatPageLoaded {
                request_id: next.request_id,
                offset: CHAT_CACHE_LIMIT,
                next_offset: CHAT_CACHE_LIMIT + CHAT_PAGE_SIZE,
                next_after: Some(limit + 50),
                has_more: true,
                chats: (limit + 1..=limit + 50).map(ranked).collect(),
            }),
        ));
        assert_eq!(state.chats.len(), CHAT_CACHE_LIMIT);
        assert_eq!(state.chats[0].id, 51);
        assert_eq!(state.chat(1).unwrap().id, 1);
        drop(update(&mut state, viewport(53, true, false)));
        let (previous, _) = state.chat_pages.pending.unwrap();
        assert_eq!(previous.anchor, Some(ChatPageAnchor::Before(51)));
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatPageLoaded {
                request_id: previous.request_id,
                offset: 0,
                next_offset: CHAT_PAGE_SIZE,
                next_after: Some(50),
                has_more: true,
                chats: (1..=50).map(ranked).collect(),
            }),
        ));
        assert_eq!(state.chat_pages.first_offset, 0);
        assert_eq!(state.chat_pages.next_offset, CHAT_CACHE_LIMIT);
        assert_eq!(state.chats.len(), CHAT_CACHE_LIMIT);
        assert_eq!(state.chats[0].id, 1);
    }

    #[test]
    fn main_list_fetches_next_page_on_scroll_and_preserves_rank_order() {
        let (mut state, _) = boot(Adapter::for_tests(), crate::config::ColorScheme::Light);
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::AuthorizationChanged(
                AuthorizationState::Ready,
            )),
        ));
        let first = state.chat_pages.pending.unwrap().0;
        assert_eq!((first.offset, first.limit), (0, CHAT_PAGE_SIZE));
        let ranked = |id: i64| {
            let mut chat = unread_chat(id);
            chat.position = 200 - id;
            chat
        };
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatPageLoaded {
                request_id: first.request_id,
                offset: 0,
                next_offset: CHAT_PAGE_SIZE,
                next_after: Some(50),
                has_more: true,
                chats: (1..=50).rev().map(ranked).collect(),
            }),
        ));
        let scroll = |remaining_y| {
            Message::ChatsScrolled(ChatListViewport {
                anchor: Some(45),
                inset: 0.0,
                row_height: 60.0,
                near_start: false,
                near_end: remaining_y <= 300.0,
            })
        };
        drop(update(&mut state, scroll(500.0)));
        assert!(state.chat_pages.pending.is_none());
        drop(update(&mut state, scroll(299.0)));
        let next = state.chat_pages.pending.unwrap().0;
        assert_eq!(
            (next.offset, next.anchor),
            (CHAT_PAGE_SIZE, Some(ChatPageAnchor::After(50)))
        );
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatPageFailed {
                request_id: next.request_id,
                message: "Could not load chats.".into(),
            }),
        ));
        drop(update(&mut state, scroll(0.0)));
        assert!(state.chat_pages.pending.is_none());
        drop(update(&mut state, Message::RetryChatPage));
        let retry = state.chat_pages.pending.unwrap().0;
        assert_ne!(retry.request_id, next.request_id);
        assert_eq!(
            (retry.offset, retry.anchor),
            (CHAT_PAGE_SIZE, Some(ChatPageAnchor::After(50)))
        );

        let mut promoted = ranked(51);
        promoted.position = 250;
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatUpdated(promoted.clone())),
        ));
        assert_eq!(state.chats.len(), CHAT_PAGE_SIZE);
        assert_eq!(state.chats.first(), Some(&promoted));

        state.open_chat_id = Some(1);
        state.forward_selection = Some(ForwardSelection {
            message_id: 42,
            destination: None,
        });
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatPageLoaded {
                request_id: retry.request_id,
                offset: CHAT_PAGE_SIZE,
                next_offset: 52,
                next_after: Some(52),
                has_more: false,
                chats: vec![ranked(52), ranked(51), ranked(50)],
            }),
        ));
        assert!(
            state
                .chats
                .iter()
                .map(|chat| chat.id)
                .eq(std::iter::once(51).chain(1..=50).chain(std::iter::once(52)))
        );
        drop(update(&mut state, Message::SelectForwardDestination(52)));
        assert_eq!(state.forward_selection().unwrap().destination, Some(52));
        assert_eq!(state.open_chat_id(), Some(1));
        drop(update(&mut state, scroll(0.0)));
        assert!(state.chat_pages.pending.is_none());
    }

    #[test]
    fn switching_chats_discards_the_previous_history_page() {
        let mut state = visual_fixture(
            crate::config::ColorScheme::Light,
            vec![unread_chat(10), unread_chat(20)],
            vec![message(50)],
        );
        state.has_older_messages = true;
        let scroll = |chat_id| {
            Message::HistoryScrolled(HistoryViewport {
                chat_id,
                offset_y: 0.0,
                height: 300.0,
                remaining_y: 1_500.0,
                direction: None,
            })
        };
        drop(update(&mut state, scroll(10)));
        let (previous, _) = state.history_request.unwrap();

        drop(update(&mut state, Message::OpenChat(20)));
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatHistoryLoaded {
                chat_id: 20,
                messages: vec![message(60)],
                has_older_messages: true,
            }),
        ));
        drop(update(&mut state, scroll(10)));
        assert_eq!(state.history_request, None);
        drop(update(&mut state, scroll(20)));
        let current = state.history_request.unwrap();
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::HistoryPageLoaded {
                request_id: previous,
                messages: vec![message(40)],
                has_more: false,
            }),
        ));
        assert_eq!(state.history_request, Some(current));
        assert_eq!(state.messages, vec![message(60)]);
        assert!(state.has_older_messages);
    }

    #[test]
    fn older_history_prefetch_retries_on_input_and_ignores_stale_results() {
        let mut state = visual_fixture(
            crate::config::ColorScheme::Light,
            vec![unread_chat(10)],
            vec![message(50)],
        );
        state.has_older_messages = true;
        let scroll = |offset_y, direction| {
            Message::HistoryScrolled(HistoryViewport {
                chat_id: 10,
                offset_y,
                height: 300.0,
                remaining_y: 0.0,
                direction,
            })
        };
        let up = Some(HistoryScrollDirection::Up);
        drop(update(&mut state, scroll(450.0, up)));
        assert_eq!(state.history_request, None);
        drop(update(&mut state, scroll(0.0, None)));
        let first = state.history_request.unwrap();
        drop(update(&mut state, scroll(0.0, up)));
        assert_eq!(state.history_request, Some(first));
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::HistoryPageFailed {
                request_id: first.0,
                message: "Try again".into(),
            }),
        ));
        drop(update(&mut state, scroll(0.0, None)));
        drop(update(
            &mut state,
            scroll(0.0, Some(HistoryScrollDirection::Down)),
        ));
        assert_eq!(state.history_request, None);

        drop(update(&mut state, scroll(0.0, up)));
        let retry = state.history_request.unwrap();
        assert_ne!(first, retry);
        assert!(state.history_error().is_none());
        for request_id in [first, retry] {
            drop(update(
                &mut state,
                Message::BackendEvent(BackendEvent::HistoryPageLoaded {
                    request_id: request_id.0,
                    messages: vec![message(40)],
                    has_more: true,
                }),
            ));
            if request_id == first {
                assert_eq!(state.history_request, Some(retry));
                assert_eq!(state.messages.len(), 1);
            }
        }
        assert_eq!(state.messages.len(), 2);
        assert!(state.has_older_messages);
        assert!(state.history_error().is_none());
        drop(update(&mut state, scroll(0.0, None)));
        let next = state.history_request.unwrap();
        drop(update(
            &mut state,
            Message::BackendEvent(BackendEvent::HistoryPageLoaded {
                request_id: next.0,
                messages: vec![],
                has_more: false,
            }),
        ));
        drop(update(&mut state, scroll(0.0, up)));
        assert_eq!(state.history_request, None);
    }

    #[test]
    fn local_original_resolves_without_a_backend_request() {
        let mut messages = [message(1), reply(2, 10, 1)];
        assert!(collect_reply_lookups(&mut messages, 10, true).is_empty());

        let preview = messages[1]
            .reply
            .as_ref()
            .unwrap()
            .preview
            .as_ref()
            .unwrap();
        assert_eq!(preview.sender_name.as_deref(), Some("Anna"));
        assert_eq!(preview.text.as_ref(), "first line second line");
    }

    #[test]
    fn cross_chat_collision_requests_the_real_original_in_bounded_batches() {
        let mut messages = vec![message(1)];
        messages.extend((2..=7).map(|id| reply(id, 20, 1)));
        assert!(collect_reply_lookups(&mut messages, 10, false).is_empty());

        let batch = collect_reply_lookups(&mut messages, 10, true);
        assert_eq!(batch, [7, 6, 5, 4]);

        apply_reply_previews(&mut messages, batch.into_iter().map(|id| (id, None)));
        assert_eq!(collect_reply_lookups(&mut messages, 10, true), [3, 2]);
    }

    #[test]
    fn reply_keeps_its_quote_when_original_is_unavailable() {
        let mut messages = [reply(2, 10, 1), reply(3, 10, 1)];
        messages[0].reply.as_mut().unwrap().quote = Some("selected quote".into());

        apply_reply_previews(&mut messages, [(2, None), (3, None)]);

        let snippet = |index: usize| messages[index].reply.as_ref().unwrap().display_text();
        assert_eq!(snippet(0), "selected quote");
        assert_eq!(snippet(1), "Original message unavailable");
    }

    #[test]
    fn edit_updates_original_and_live_preview_but_preserves_quote() {
        let mut messages = [
            message(1),
            reply(2, 10, 1),
            reply(3, 10, 1),
            reply(4, 20, 1),
        ];
        messages[2].reply.as_mut().unwrap().quote = Some("original quote".into());
        collect_reply_lookups(&mut messages, 10, false);

        apply_message_content(
            &mut messages,
            Some(10),
            MessageAddress {
                chat_id: 10,
                message_id: 1,
            },
            "updated text".into(),
            Box::default(),
            true,
        );

        assert_eq!(messages[0].text, "updated text");
        assert_eq!(
            messages[1].reply.as_ref().unwrap().display_text(),
            "updated text"
        );
        assert_eq!(
            messages[2].reply.as_ref().unwrap().display_text(),
            "original quote"
        );
        assert!(messages[3].reply.as_ref().unwrap().preview.is_none());
    }

    #[test]
    fn deletion_updates_only_the_target_rows_and_reply_references() {
        let mut messages = vec![message(1), reply(2, 20, 1), reply(3, 10, 1)];
        apply_message_deletion(&mut messages, Some(10), 20, |id| id == 1);
        assert_eq!(messages.len(), 3);
        assert_eq!(
            messages[1].reply.as_ref().unwrap().display_text(),
            "Original message unavailable"
        );
        assert!(messages[2].reply.as_ref().unwrap().preview.is_none());

        apply_message_deletion(&mut messages, Some(10), 10, |id| id == 1);
        assert_eq!(
            messages
                .iter()
                .map(|message| message.id)
                .collect::<Vec<_>>(),
            [2, 3]
        );
        assert_eq!(
            messages[1].reply.as_ref().unwrap().display_text(),
            "Original message unavailable"
        );
    }

    fn conversation_with_own_message(actions: MessageActions) -> State {
        let (mut state, _) = boot(Adapter::for_tests(), Default::default());
        state.open_chat_id = Some(10);
        let mut own = message(1);
        own.is_outgoing = true;
        state.messages.push(own);
        let _ = update(
            &mut state,
            Message::OpenMessageMenu {
                chat_id: 10,
                message_id: 1,
                position: Point::ORIGIN,
            },
        );
        let request_id = state.message_menu().unwrap().request_id.unwrap();
        let _ = update(
            &mut state,
            Message::BackendEvent(BackendEvent::MessageActionsLoaded {
                request_id,
                result: Ok(actions),
            }),
        );
        state
    }

    #[test]
    fn rejected_edit_keeps_its_text_and_cancel_restores_the_draft() {
        let mut state = conversation_with_own_message(MessageActions {
            can_edit: true,
            ..MessageActions::default()
        });
        state.composer = text_editor::Content::with_text("unsent draft");
        let _ = update(&mut state, Message::StartEdit(1));
        assert_eq!(state.composer.text(), state.messages[0].text);

        state.composer = text_editor::Content::with_text("edited text");
        let history_end_revision = state.history_end_revision();
        let _ = update(&mut state, Message::Submit);
        assert_eq!(state.history_end_revision(), history_end_revision);
        let request_id = state.pending_mutation.as_ref().unwrap().request_id;
        let _ = update(
            &mut state,
            Message::BackendEvent(BackendEvent::MessageMutationFinished {
                request_id,
                result: Err("edit rejected".into()),
            }),
        );
        assert_eq!(state.composer.text(), "edited text");
        assert_eq!(state.message_action_error(), Some("edit rejected"));

        let _ = update(&mut state, Message::CancelEdit);
        assert_eq!(state.composer.text(), "unsent draft");
        assert!(!state.is_editing());
    }

    #[test]
    fn deletion_requires_confirmation_for_a_permitted_scope() {
        let mut state = conversation_with_own_message(MessageActions {
            can_delete_for_self: true,
            ..MessageActions::default()
        });
        let _ = update(&mut state, Message::ConfirmDelete(DeleteScope::ForSelf));
        assert!(!state.mutation_pending());

        let _ = update(&mut state, Message::ShowDeleteConfirmation(1));
        assert!(state.delete_confirmation().is_some());
        let _ = update(&mut state, Message::ConfirmDelete(DeleteScope::ForEveryone));
        assert!(!state.mutation_pending());
        let _ = update(&mut state, Message::ConfirmDelete(DeleteScope::ForSelf));
        assert!(state.mutation_pending());
        assert_eq!(state.messages.len(), 1);

        let _ = update(
            &mut state,
            Message::BackendEvent(BackendEvent::MessagesDeleted {
                chat_id: 10,
                message_ids: vec![1],
            }),
        );
        assert!(state.messages.is_empty());
        assert!(state.delete_confirmation().is_none());
    }

    fn unread_chat(id: i64) -> ChatSummary {
        ChatSummary {
            id,
            title: "Chat".into(),
            last_message_id: Some(5),
            last_message_date: Some(60),
            last_message_is_outgoing: false,
            last_message_preview: "latest".into(),
            unread_count: 4,
            last_read_inbox_message_id: 1,
            last_read_outbox_message_id: Some(0),
            position: 1,
            can_receive_forwards: true,
        }
    }

    fn unread_conversation() -> State {
        let (mut state, _) = boot(Adapter::for_tests(), Default::default());
        state.authorization_state = Some(AuthorizationState::Ready);
        state.open_chat_id = Some(10);
        state.chats.push(unread_chat(10));
        state.messages = (2..=5).map(message).collect();
        state
    }

    fn viewed(state: &mut State, chat_id: i64, message_id: i64) {
        let _ = update(
            state,
            Message::HistoryReadBoundary(MessageAddress {
                chat_id,
                message_id,
            }),
        );
    }

    fn finish_read(state: &mut State, accepted: bool) {
        let request_id = state.read_in_flight.as_ref().unwrap().request_id;
        let _ = update(
            state,
            Message::BackendEvent(BackendEvent::ReadReceiptFinished {
                request_id,
                accepted,
            }),
        );
    }

    #[test]
    fn active_chat_coalesces_viewed_boundaries_without_guessing_unread_count() {
        let mut state = unread_conversation();
        let _ = update(&mut state, Message::WindowFocusChanged(true));

        viewed(&mut state, 10, 2);
        viewed(&mut state, 10, 2);
        viewed(&mut state, 10, 3);
        viewed(&mut state, 10, 5);
        assert_eq!(state.read_in_flight.as_ref().unwrap().address.message_id, 2);
        finish_read(&mut state, true);
        assert_eq!(state.read_in_flight.as_ref().unwrap().address.message_id, 5);

        finish_read(&mut state, true);
        viewed(&mut state, 10, 5);
        assert!(state.read_in_flight.is_none());
        assert_eq!(state.chats[0].unread_count, 4);

        let _ = update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatInboxRead {
                chat_id: 10,
                last_read_message_id: 5,
                unread_count: 0,
            }),
        );
        let _ = update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatInboxRead {
                chat_id: 10,
                last_read_message_id: 2,
                unread_count: 3,
            }),
        );
        assert_eq!(state.chats[0].unread_count, 0);
        assert_eq!(state.chats[0].last_read_inbox_message_id, 5);
    }

    #[test]
    fn background_window_and_stale_chat_geometry_do_not_mark_messages() {
        let mut state = unread_conversation();
        viewed(&mut state, 10, 5);
        assert!(state.read_in_flight.is_none());

        let _ = update(&mut state, Message::WindowFocusChanged(true));
        viewed(&mut state, 10, 2);
        viewed(&mut state, 10, 5);
        let _ = update(&mut state, Message::WindowFocusChanged(false));
        finish_read(&mut state, true);
        assert!(state.read_in_flight.is_none());

        let _ = update(&mut state, Message::WindowFocusChanged(true));
        let _ = update(&mut state, Message::OpenChat(20));
        viewed(&mut state, 10, 5);
        assert!(state.read_in_flight.is_none());
    }

    #[test]
    fn failed_receipt_waits_for_a_fresh_focus_or_newer_message() {
        let mut state = unread_conversation();
        let _ = update(&mut state, Message::WindowFocusChanged(true));
        viewed(&mut state, 10, 5);
        finish_read(&mut state, false);
        viewed(&mut state, 10, 5);
        assert!(state.read_in_flight.is_none());

        let _ = update(&mut state, Message::WindowFocusChanged(false));
        let _ = update(&mut state, Message::WindowFocusChanged(true));
        viewed(&mut state, 10, 5);
        assert!(state.read_in_flight.is_some());
    }

    #[test]
    fn outbox_read_boundary_advances_only_for_the_reported_chat() {
        let mut state = unread_conversation();
        let mut unsupported = unread_chat(20);
        unsupported.last_read_outbox_message_id = None;
        state.chats.push(unsupported);
        for (chat_id, last_read_message_id) in [(10, 4), (10, 2), (20, 5)] {
            let _ = update(
                &mut state,
                Message::BackendEvent(BackendEvent::ChatOutboxRead {
                    chat_id,
                    last_read_message_id,
                }),
            );
        }
        let _ = update(
            &mut state,
            Message::BackendEvent(BackendEvent::ChatUpdated(unread_chat(10))),
        );

        let private_chat = state.chats.iter().find(|chat| chat.id == 10).unwrap();
        assert_eq!(private_chat.last_read_outbox_message_id, Some(4));
        assert_eq!(private_chat.unread_count, 4);
        let unsupported = state.chats.iter().find(|chat| chat.id == 20).unwrap();
        assert_eq!(unsupported.last_read_outbox_message_id, None);
    }

    #[test]
    fn live_content_updates_replace_styles_even_when_text_is_unchanged() {
        let (mut state, _) = boot(Adapter::for_tests(), Default::default());
        state.open_chat_id = Some(10);
        let mut original = message(1);
        original.text = "unchanged".into();
        original.text_runs = Box::new([TextRun {
            end: 9,
            style: TextStyle::BOLD,
        }]);
        state.messages.push(original);

        let updates: [Box<[TextRun]>; 2] = [
            Box::new([TextRun {
                end: 9,
                style: TextStyle::ITALIC,
            }]),
            Box::default(),
        ];
        for text_runs in updates {
            let _ = update(
                &mut state,
                Message::BackendEvent(BackendEvent::MessageContentChanged {
                    address: MessageAddress {
                        chat_id: 10,
                        message_id: 1,
                    },
                    text: "unchanged".into(),
                    text_runs: text_runs.clone(),
                    is_text: true,
                }),
            );
            assert_eq!(state.messages[0].text_runs, text_runs);
            assert_eq!(state.messages[0].text, "unchanged");
        }
    }

    #[test]
    fn revealing_a_spoiler_keeps_previews_masked_and_an_edit_hides_it_again() {
        let (mut state, _) = boot(Adapter::for_tests(), Default::default());
        state.open_chat_id = Some(10);
        let mut chat = unread_chat(10);
        chat.last_message_id = Some(1);
        state.chats.push(chat);
        state.messages = vec![message(1), reply(2, 10, 1)];
        let runs = || {
            Box::<[TextRun]>::from([
                TextRun {
                    end: 5,
                    style: TextStyle::default(),
                },
                TextRun {
                    end: 8,
                    style: TextStyle::SPOILER,
                },
            ])
        };
        let address = MessageAddress {
            chat_id: 10,
            message_id: 1,
        };
        let change = |text: &str| {
            Message::BackendEvent(BackendEvent::MessageContentChanged {
                address,
                text: text.into(),
                text_runs: runs(),
                is_text: true,
            })
        };
        let _ = update(&mut state, change("hint one"));
        let _ = update(
            &mut state,
            Message::RevealSpoilers(MessageAddress {
                chat_id: 20,
                ..address
            }),
        );
        assert!(!state.messages[0].spoilers_revealed);

        let _ = update(&mut state, Message::RevealSpoilers(address));
        assert!(state.messages[0].spoilers_revealed);
        assert_eq!(state.chats[0].last_message_preview, "hint •••");
        assert_eq!(
            state.messages[1].reply.as_ref().unwrap().display_text(),
            "hint •••"
        );
        assert_eq!(
            MessagePreview::message_snippet(&state.messages[0].text, &state.messages[0].text_runs)
                .as_ref(),
            "hint •••"
        );

        let _ = update(&mut state, change("hint two"));
        assert!(!state.messages[0].spoilers_revealed);
        assert_eq!(state.messages[0].text, "hint two");
        assert_eq!(state.chats[0].last_message_preview, "hint •••");
        let _ = update(&mut state, Message::RevealSpoilers(address));
        let _ = update(&mut state, Message::OpenChat(20));
        assert!(state.messages.is_empty());
    }
}
