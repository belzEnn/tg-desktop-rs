// Application state and update logic.

use iced::widget::text_editor;
use iced::{Subscription, Task};
use std::collections::HashSet;

use crate::{
    domain::{AuthorizationState, BackendEvent, ChatMessage, ChatSummary},
    tg::{Adapter, AdapterCommandError},
};

const HISTORY_PAGE_SIZE: usize = 50;
const HISTORY_PREFETCH_AFTER_MESSAGES: usize = 30;
const HISTORY_PREFETCH_FRACTION: f32 =
    (HISTORY_PAGE_SIZE - HISTORY_PREFETCH_AFTER_MESSAGES) as f32 / HISTORY_PAGE_SIZE as f32;
pub(crate) const CHAT_HISTORY_SCROLL_ID: &str = "chat-history";

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
    };
    let task = Task::perform(
        async move { backend.initialize().await },
        Message::Initialized,
    );
    (state, task)
}

/// Subscribes the application to backend events.
pub(crate) fn subscription(state: &State) -> Subscription<Message> {
    state.backend.subscription().map(Message::BackendEvent)
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
                state.messages.clear();
                state.has_older_messages = false;
                state.loading_older_messages = false;
                state.history_load_trigger_armed = false;
                state.history_content_height = 0.0;
                state.pending_scroll_adjustment = None;
                state.composer = text_editor::Content::new();
                state.send_error = None;
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
                state.messages = messages;
                state.history_loading = false;
                state.history_error = None;
                state.has_older_messages = has_older_messages;
                state.loading_older_messages = false;
                state.history_load_trigger_armed = false;
                state.pending_scroll_adjustment = None;
                return iced::widget::operation::snap_to_end(CHAT_HISTORY_SCROLL_ID);
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
            }
        }
        Message::BackendEvent(BackendEvent::MessageSendSucceeded {
            chat_id,
            old_message_id,
            message,
        }) => {
            if state.open_chat_id == Some(chat_id) {
                state
                    .messages
                    .retain(|existing| existing.id != old_message_id);
                insert_message(&mut state.messages, message);
            }
        }
        Message::BackendEvent(BackendEvent::MessageSendRejected {
            chat_id,
            message_id,
            text,
            reason,
        }) => {
            if state.open_chat_id == Some(chat_id) {
                state.messages.retain(|existing| existing.id != message_id);
                if state.composer.text().is_empty() {
                    state.composer = text_editor::Content::with_text(&text);
                }
                state.send_error = Some(reason);
            }
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
            if state.open_chat_id != Some(chat_id) {
                state.open_chat_id = Some(chat_id);
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
        Message::Submit => {
            let Some(chat_id) = state.open_chat_id else {
                return Task::none();
            };
            let text = state.composer.text();
            if text.trim().is_empty() {
                return Task::none();
            }
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
                },
            );
            state.composer = text_editor::Content::new();
            state.send_error = None;
            let backend = state.backend.clone();
            return Task::perform(
                async move {
                    let result = backend.send_text(chat_id, message_id, text.clone()).await;
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
                state.messages.retain(|message| message.id != message_id);
                if state.composer.text().is_empty() {
                    state.composer = text_editor::Content::with_text(&text);
                }
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
    pub(crate) fn composer(&self) -> &text_editor::Content {
        &self.composer
    }
    pub(crate) fn send_error(&self) -> Option<&str> {
        self.send_error.as_deref()
    }
}
