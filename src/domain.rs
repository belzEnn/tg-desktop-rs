// Shared application types, independent of the backend and UI.

/// Telegram authorization progress, independent of the selected backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationState {
    WaitTdlibParameters,
    WaitPhoneNumber,
    WaitCode,
    WaitPassword,
    WaitRegistration,
    Ready,
    Closed,
    Other,
}

impl AuthorizationState {
    /// Returns a stable, non-sensitive label for logs and status displays.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WaitTdlibParameters => "wait_tdlib_parameters",
            Self::WaitPhoneNumber => "wait_phone_number",
            Self::WaitCode => "wait_code",
            Self::WaitPassword => "wait_password",
            Self::WaitRegistration => "wait_registration",
            Self::Ready => "ready",
            Self::Closed => "closed",
            Self::Other => "other",
        }
    }
}

/// An event emitted by the Telegram backend adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendEvent {
    AuthorizationChanged(AuthorizationState),
    /// A safe-to-display error from an authorization request.
    AuthorizationError(String),
    /// A chat in the main list was added or updated.
    ChatUpdated(ChatSummary),
    /// A chat was removed from the main list.
    ChatRemoved(i64),
    /// The requested chat history was loaded.
    ChatHistoryLoaded {
        chat_id: i64,
        messages: Vec<ChatMessage>,
    },
    /// A newly received or sent message in a chat.
    NewMessage {
        chat_id: i64,
        message: ChatMessage,
    },
    /// A chat history request failed with a safe-to-display message.
    ChatHistoryError {
        chat_id: i64,
        message: String,
    },
}

/// A message row with backend-independent display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: i64,
    pub date: i32,
    pub sender_name: String,
    pub text: String,
    pub is_outgoing: bool,
}

/// The small amount of chat data needed to render one main-list row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSummary {
    pub id: i64,
    pub title: String,
    pub last_message_preview: String,
    pub unread_count: u32,
    pub position: i64,
}
