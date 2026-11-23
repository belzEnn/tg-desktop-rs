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
        AuthorizationState, BackendEvent, ChatMessage, ChatSummary, MessagePreview, MessageReply,
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
    backend: Adapter,
    authorization_state: Option<AuthorizationState>,
    phone: String,
    code: String,
    password: String,
    authorization_error: Option<String>,
    logout_pending: bool,
    chats: Vec<ChatSummary>,
    open_chat_id: Option<i64>,
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
pub(crate) fn boot(backend: Adapter) -> (State, Task<Message>) {
    let state = State {
        backend: backend.clone(),
        authorization_state: None,
        phone: String::new(),
        code: String::new(),
        password: String::new(),
        authorization_error: None,
        logout_pending: false,
        chats: Vec::new(),
        open_chat_id: None,
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
    let backend = state.backend.subscription().map(Message::BackendEvent);
    if state.message_menu.is_some() {
        Subscription::batch([backend, event::listen_with(message_menu_event)])
    } else {
        backend
    }
}

fn message_menu_event(event: Event, status: event::Status, _: window::Id) -> Option<Message> {
    match event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(Named::Escape),
            ..
        }) => Some(Message::DismissMessageMenu),
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
            if status == event::Status::Ignored =>
        {
            Some(Message::DismissMessageMenu)
        }
        Event::Mouse(mouse::Event::WheelScrolled { .. })
        | Event::Window(window::Event::Unfocused | window::Event::Resized(_)) => {
            Some(Message::DismissMessageMenu)
        }
        _ => None,
    }
}

/// Applies backend events and command results to application state.
pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::BackendEvent(BackendEvent::AuthorizationChanged(authorization_state)) => {
            if authorization_state == AuthorizationState::Closed {
                state.authorization_state = None;
                state.authorization_error = None;
                state.logout_pending = false;
                state.chats.clear();
                state.open_chat_id = None;
                state.message_menu = None;
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
        Message::BackendEvent(BackendEvent::ChatUpdated(chat)) => {
            if let Some(existing) = state
                .chats
                .iter_mut()
                .find(|existing| existing.id == chat.id)
            {
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
            state.composer.perform(action);
            state.send_error = None;
        }
        Message::OpenMessageMenu {
            chat_id,
            message_id,
            position,
        } => {
            if state.open_chat_id == Some(chat_id)
                && state.referenceable_message(message_id).is_some()
            {
                state.message_menu = Some(MessageMenu {
                    message_id,
                    position,
                });
            }
        }
        Message::DismissMessageMenu => state.message_menu = None,
        Message::SelectReply(message_id) => {
            state.message_menu = None;
            if state.referenceable_message(message_id).is_some() {
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
            let Some(chat_id) = state.open_chat_id else {
                return Task::none();
            };
            let text = state.composer.text();
            if text.trim().is_empty() {
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
                    is_outgoing: true,
                    can_reference: false,
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
            let preview = MessagePreview::from_text(&original.sender_name, &original.text);
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
    if state.composer.text().is_empty() && state.reply_to.is_none() {
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
            is_outgoing: false,
            can_reference: true,
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

        let snippet = |index: usize| {
            messages[index]
                .reply
                .as_ref()
                .unwrap()
                .preview
                .as_ref()
                .unwrap()
                .text
                .as_ref()
        };
        assert_eq!(snippet(0), "selected quote");
        assert_eq!(snippet(1), "Original message unavailable");
    }
}
