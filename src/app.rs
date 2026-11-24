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
        AuthorizationState, BackendEvent, ChatMessage, ChatSummary, DeleteScope, MessageAction,
        MessageActions, MessageAddress, MessagePreview, MessageReply, TextRun, TextStyle,
        conceal_spoilers,
    },
    tg::{Adapter, AdapterCommandError},
};

const REPLY_LOOKUP_BATCH_SIZE: usize = 4;
const HISTORY_PAGE_SIZE: usize = 50;
const HISTORY_PREFETCH_AFTER_MESSAGES: usize = 30;
const HISTORY_PREFETCH_FRACTION: f32 =
    (HISTORY_PAGE_SIZE - HISTORY_PREFETCH_AFTER_MESSAGES) as f32 / HISTORY_PAGE_SIZE as f32;
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
    open_chat_id: Option<i64>,
    window_focused: bool,
    history_read: HistoryRead,
    read_in_flight: Option<PendingRead>,
    messages: Vec<ChatMessage>,
    history_loading: bool,
    history_error: Option<String>,
    has_older_messages: bool,
    loading_older_messages: bool,
    history_load_trigger_armed: bool,
    history_content_height: f32,
    pending_scroll_adjustment: Option<f32>,
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

/// The message and window position of an open context menu.
#[derive(Clone, Copy)]
pub(crate) struct MessageMenu {
    pub message_id: i64,
    pub position: Point,
    pub actions: MessageActions,
    pub request_id: Option<u64>,
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
    WindowFocusChanged(bool),
    HistoryReadBoundary(MessageAddress),
    ReadReceiptQueued {
        request_id: u64,
        result: Result<(), AdapterCommandError>,
    },
    ChatOpenCommandSent(Result<(), AdapterCommandError>),
    HistoryScrolled {
        offset_y: f32,
        content_height: f32,
    },
    OlderMessagesCommandSent {
        chat_id: i64,
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
        open_chat_id: None,
        window_focused: false,
        history_read: HistoryRead::default(),
        read_in_flight: None,
        messages: Vec::new(),
        history_loading: false,
        history_error: None,
        has_older_messages: false,
        loading_older_messages: false,
        history_load_trigger_armed: false,
        history_content_height: 0.0,
        pending_scroll_adjustment: None,
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
                state.open_chat_id = None;
                state.history_read = HistoryRead::default();
                state.read_in_flight = None;
                state.message_menu = None;
                state.edit = None;
                state.delete_confirmation = None;
                state.pending_mutation = None;
                state.message_action_error = None;
                state.messages.clear();
                state.has_older_messages = false;
                state.loading_older_messages = false;
                state.history_load_trigger_armed = false;
                state.history_content_height = 0.0;
                state.pending_scroll_adjustment = None;
                state.composer = text_editor::Content::new();
                state.send_error = None;
                state.reply_to = None;
                state.forward_selection = None;
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
        }
        Message::BackendEvent(BackendEvent::AuthorizationError(error)) => {
            state.authorization_error = Some(error);
            state.password.clear();
            state.logout_pending = false;
        }
        Message::BackendEvent(BackendEvent::ChatUpdated(mut chat)) => {
            if let Some(existing) = state
                .chats
                .iter_mut()
                .find(|existing| existing.id == chat.id)
            {
                // A snapshot can overtake a queued inbox or outbox update.
                if chat.last_read_inbox_message_id < existing.last_read_inbox_message_id {
                    chat.last_read_inbox_message_id = existing.last_read_inbox_message_id;
                    chat.unread_count = existing.unread_count;
                }
                if let (Some(read), Some(previous)) = (
                    &mut chat.last_read_outbox_message_id,
                    existing.last_read_outbox_message_id,
                ) {
                    *read = (*read).max(previous);
                }
                *existing = chat;
            } else {
                state.chats.push(chat);
            }
            state.chats.sort_by(|left, right| {
                right
                    .position
                    .cmp(&left.position)
                    .then_with(|| right.id.cmp(&left.id))
            });
            state.chats.truncate(50);
        }
        Message::BackendEvent(BackendEvent::ChatInboxRead {
            chat_id,
            last_read_message_id,
            unread_count,
        }) => {
            if let Some(chat) = state.chats.iter_mut().find(|chat| chat.id == chat_id)
                && last_read_message_id >= chat.last_read_inbox_message_id
            {
                chat.last_read_inbox_message_id = last_read_message_id;
                chat.unread_count = unread_count;
            }
        }
        Message::BackendEvent(BackendEvent::ChatOutboxRead {
            chat_id,
            last_read_message_id,
        }) => {
            if let Some(chat) = state.chats.iter_mut().find(|chat| chat.id == chat_id)
                && let Some(read) = &mut chat.last_read_outbox_message_id
            {
                *read = (*read).max(last_read_message_id);
            }
        }
        Message::BackendEvent(BackendEvent::ReadReceiptFinished {
            request_id,
            accepted,
        }) => {
            return finish_read_receipt(state, request_id, accepted);
        }
        Message::BackendEvent(BackendEvent::ChatRemoved(chat_id)) => {
            state.chats.retain(|chat| chat.id != chat_id);
        }
        Message::BackendEvent(BackendEvent::ChatHistoryLoaded {
            chat_id,
            messages,
            has_older_messages,
        }) => {
            if state.open_chat_id == Some(chat_id) {
                state.message_menu = None;
                state.messages = messages;
                state.history_loading = false;
                state.history_error = None;
                state.has_older_messages = has_older_messages;
                state.loading_older_messages = false;
                state.history_load_trigger_armed = false;
                state.pending_scroll_adjustment = None;
                return Task::batch([
                    iced::widget::operation::snap_to_end(CHAT_HISTORY_SCROLL_ID),
                    refresh_reply_previews(state),
                ]);
            }
        }
        Message::BackendEvent(BackendEvent::OlderChatHistoryLoaded {
            chat_id,
            messages,
            has_older_messages,
        }) => {
            if state.open_chat_id == Some(chat_id) {
                state.loading_older_messages = false;
                state.has_older_messages = has_older_messages;
                state.history_error = None;
                if !messages.is_empty() {
                    state.pending_scroll_adjustment = Some(state.history_content_height);
                    prepend_messages(&mut state.messages, messages);
                    return refresh_reply_previews(state);
                } else {
                    state.pending_scroll_adjustment = None;
                }
            }
        }
        Message::BackendEvent(BackendEvent::OlderChatHistoryError { chat_id, message }) => {
            if state.open_chat_id == Some(chat_id) {
                state.loading_older_messages = false;
                state.pending_scroll_adjustment = None;
                state.history_error = Some(message);
            }
        }
        Message::BackendEvent(BackendEvent::ChatHistoryError { chat_id, message }) => {
            if state.open_chat_id == Some(chat_id) {
                state.history_loading = false;
                state.history_error = Some(message);
            }
        }
        Message::BackendEvent(BackendEvent::NewMessage { chat_id, message }) => {
            if state.open_chat_id == Some(chat_id) {
                insert_message(&mut state.messages, message);
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
                state.messages.retain(|existing| {
                    existing.id != old_message_id && Some(existing.id) != local_message_id
                });
                insert_message(&mut state.messages, message);
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
            if let Some(chat) = state.chats.iter_mut().find(|chat| {
                chat.id == address.chat_id && chat.last_message_id == Some(address.message_id)
            }) {
                chat.last_message_preview = conceal_spoilers(&text, &text_runs).into_owned();
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
        Message::OpenChat(chat_id) => {
            state.message_menu = None;
            if state.open_chat_id != Some(chat_id) {
                state.open_chat_id = Some(chat_id);
                state.history_read = HistoryRead::default();
                state.edit = None;
                state.delete_confirmation = None;
                state.message_action_error = None;
                state.reply_to = None;
                state.forward_selection = None;
                state.forward_status = None;
                state.messages.clear();
                state.has_older_messages = false;
                state.loading_older_messages = false;
                state.history_load_trigger_armed = false;
                state.history_content_height = 0.0;
                state.pending_scroll_adjustment = None;
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
            if let Some(chat) = state.chats.iter().find(|chat| chat.id == address.chat_id)
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
        Message::HistoryScrolled {
            offset_y,
            content_height,
        } => {
            state.message_menu = None;
            state.history_content_height = content_height;
            if let Some(previous_content_height) = state.pending_scroll_adjustment.take() {
                let added_height = (content_height - previous_content_height).max(0.0);
                if added_height > 0.0 {
                    return iced::widget::operation::scroll_by(
                        CHAT_HISTORY_SCROLL_ID,
                        iced::widget::operation::AbsoluteOffset {
                            x: 0.0,
                            y: added_height,
                        },
                    );
                }
            }

            let near_prefetch_point =
                content_height > 0.0 && offset_y / content_height <= HISTORY_PREFETCH_FRACTION;
            if !near_prefetch_point {
                state.history_load_trigger_armed = true;
            } else if state.history_load_trigger_armed
                && state.has_older_messages
                && !state.loading_older_messages
            {
                let Some(oldest_message_id) = state.messages.first().map(|message| message.id)
                else {
                    return Task::none();
                };
                let Some(chat_id) = state.open_chat_id else {
                    return Task::none();
                };
                state.history_load_trigger_armed = false;
                state.loading_older_messages = true;
                let backend = state.backend.clone();
                return Task::perform(
                    async move {
                        backend
                            .load_older_messages(chat_id, oldest_message_id)
                            .await
                    },
                    move |result| Message::OlderMessagesCommandSent { chat_id, result },
                );
            }
        }
        Message::OlderMessagesCommandSent { chat_id, result } => {
            if let Err(error) = result
                && state.open_chat_id == Some(chat_id)
            {
                state.loading_older_messages = false;
                state.history_error = Some(error.to_string());
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
            if state.can_forward_to(chat_id)
                && let Some(selection) = &mut state.forward_selection
            {
                selection.destination = Some(chat_id);
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
            insert_message(
                &mut state.messages,
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
            state.composer = text_editor::Content::new();
            state.send_error = None;
            let backend = state.backend.clone();
            return Task::perform(
                async move {
                    let result = backend
                        .send_text(chat_id, message_id, text.clone(), reply_to)
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

fn request_read_receipt(state: &mut State) -> Task<Message> {
    let Some(chat) = state
        .chats
        .iter()
        .find(|chat| Some(chat.id) == state.open_chat_id)
    else {
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
    if state.edit.is_none() && state.composer.text().is_empty() && state.reply_to.is_none() {
        state.reply_to = state
            .messages
            .iter()
            .find(|message| message.id == message_id)
            .and_then(|message| message.reply.as_ref())
            .map(|reply| reply.message_id);
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

fn prepend_messages(existing: &mut Vec<ChatMessage>, older: Vec<ChatMessage>) {
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
    fn can_mark_read(&self, chat: &ChatSummary) -> bool {
        self.authorization_state == Some(AuthorizationState::Ready)
            && self.window_focused
            && self.open_chat_id == Some(chat.id)
            && !self.logout_pending
            && !self.history_loading
            && self.pending_scroll_adjustment.is_none()
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
    pub(crate) fn history_error(&self) -> Option<&str> {
        self.history_error.as_deref()
    }
    fn referenceable_message(&self, message_id: i64) -> Option<&ChatMessage> {
        self.messages
            .iter()
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
                .chats
                .iter()
                .any(|chat| chat.id == chat_id && chat.can_receive_forwards)
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
        let _ = update(&mut state, Message::Submit);
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
