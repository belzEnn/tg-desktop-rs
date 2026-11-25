// Shared application types, independent of the backend and UI.

use std::borrow::Cow;

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
    /// A ranked page of main-list summaries from the backend.
    ChatPageLoaded {
        request_id: u64,
        next_offset: usize,
        next_after: Option<i64>,
        has_more: bool,
        chats: Vec<ChatSummary>,
    },
    /// A main-list page could not be loaded.
    ChatPageFailed {
        request_id: u64,
        message: String,
    },
    /// The backend lost some list updates and needs to refresh visible dialogs.
    ChatListInvalidated,
    /// A chat was removed from the main list.
    ChatRemoved(i64),
    /// TDLib's latest read boundary and unread count for a chat.
    ChatInboxRead {
        chat_id: i64,
        last_read_message_id: i64,
        unread_count: u32,
    },
    /// TDLib advanced the read boundary for outgoing messages.
    ChatOutboxRead {
        chat_id: i64,
        last_read_message_id: i64,
    },
    /// TDLib accepted or rejected a viewed-message notification.
    ReadReceiptFinished {
        request_id: u64,
        accepted: bool,
    },
    /// The requested chat history was loaded.
    ChatHistoryLoaded {
        chat_id: i64,
        messages: Vec<ChatMessage>,
        has_older_messages: bool,
    },
    /// An older page of messages was loaded for the active chat.
    OlderChatHistoryLoaded {
        request_id: u64,
        messages: Vec<ChatMessage>,
        has_older_messages: bool,
    },
    /// Loading an older page failed with a safe-to-display message.
    OlderChatHistoryError {
        request_id: u64,
        message: String,
    },
    /// A newly received or sent message in a chat.
    NewMessage {
        chat_id: i64,
        message: ChatMessage,
    },
    /// A temporary outgoing message was replaced by Telegram's sent message.
    MessageSendSucceeded {
        chat_id: i64,
        old_message_id: i64,
        local_message_id: Option<i64>,
        message: ChatMessage,
    },
    /// Telegram rejected an outgoing message before accepting it for delivery.
    MessageSendRejected {
        chat_id: i64,
        message_id: i64,
        text: String,
        reason: String,
    },
    /// Permissions for the message whose context menu was opened.
    MessageActionsLoaded {
        request_id: u64,
        result: Result<MessageActions, String>,
    },
    /// Telegram finished a requested edit or deletion.
    MessageMutationFinished {
        request_id: u64,
        result: Result<(), String>,
    },
    /// The display content of a message changed on Telegram.
    MessageContentChanged {
        address: MessageAddress,
        text: String,
        text_runs: Box<[TextRun]>,
        is_text: bool,
    },
    /// Telegram marked a message as edited.
    MessageEdited(MessageAddress),
    /// Messages were deleted or became inaccessible.
    MessagesDeleted {
        chat_id: i64,
        message_ids: Vec<i64>,
    },
    /// A bounded batch of previews resolved for the active conversation.
    ReplyPreviewsLoaded {
        chat_id: i64,
        previews: Vec<(i64, Option<MessagePreview>)>,
    },
    /// Telegram confirmed a forward or rejected it.
    ForwardFinished {
        result: Result<(), String>,
        discarded_message: Option<(i64, i64)>,
    },
    /// A chat history request failed with a safe-to-display message.
    ChatHistoryError {
        chat_id: i64,
        message: String,
    },
}

/// Describes a ranked range of main-list chats requested by the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChatPageQuery {
    pub request_id: u64,
    pub offset: usize,
    pub limit: usize,
    pub after: Option<i64>,
    pub load_missing: bool,
}

/// Identifies a message within its chat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageAddress {
    pub chat_id: i64,
    pub message_id: i64,
}

/// Actions Telegram currently allows for a message.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MessageActions {
    pub can_edit: bool,
    pub can_delete_for_self: bool,
    pub can_delete_for_everyone: bool,
}

/// The recipients for whom a message should be deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteScope {
    ForSelf,
    ForEveryone,
}

/// A request to inspect or change a message.
pub enum MessageAction {
    Inspect,
    Edit(String),
    Delete(DeleteScope),
}

/// Formatting flags that can apply to the same piece of message text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TextStyle(u8);

impl TextStyle {
    pub const BOLD: Self = Self(1 << 0);
    pub const ITALIC: Self = Self(1 << 1);
    pub const UNDERLINE: Self = Self(1 << 2);
    pub const STRIKETHROUGH: Self = Self(1 << 3);
    pub const CODE: Self = Self(1 << 4);
    pub const PRE: Self = Self(1 << 5);
    pub const LINK: Self = Self(1 << 6);
    pub const SPOILER: Self = Self(1 << 7);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// A contiguous UTF-8 text segment. Its start is zero or the end of the previous segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextRun {
    pub end: usize,
    pub style: TextStyle,
}

/// A message row with backend-independent display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub id: i64,
    pub date: i32,
    pub sender_name: String,
    pub text: String,
    /// Empty for plain text. Otherwise covers the full text in byte order.
    pub text_runs: Box<[TextRun]>,
    pub is_outgoing: bool,
    pub can_reference: bool,
    pub is_text: bool,
    pub is_edited: bool,
    pub spoilers_revealed: bool,
    pub reply: Option<Box<MessageReply>>,
    pub forwarded_from: Option<Box<str>>,
}

/// Replaces hidden characters while keeping whitespace and line breaks.
pub fn spoiler_mask(fragment: &str) -> String {
    masked_characters(fragment).collect()
}

fn masked_characters(fragment: &str) -> impl Iterator<Item = char> + '_ {
    fragment
        .chars()
        .map(|ch| if ch.is_whitespace() { ch } else { '•' })
}

/// Masks all spoilers in a preview regardless of their state in the history.
pub fn conceal_spoilers<'a>(text: &'a str, runs: &[TextRun]) -> Cow<'a, str> {
    if !runs
        .iter()
        .any(|run| run.style.contains(TextStyle::SPOILER))
    {
        return Cow::Borrowed(text);
    }
    let mut masked = String::new();
    let mut start = 0;
    for run in runs {
        let Some(fragment) = text.get(start..run.end) else {
            return Cow::Borrowed("•••");
        };
        if run.style.contains(TextStyle::SPOILER) {
            masked.extend(masked_characters(fragment));
        } else {
            masked.push_str(fragment);
        }
        start = run.end;
    }
    if start != text.len() {
        return Cow::Borrowed("•••");
    }
    Cow::Owned(masked)
}

/// The address and display data of a reply's original message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageReply {
    pub chat_id: i64,
    pub message_id: i64,
    pub quote: Option<Box<str>>,
    pub preview: Option<MessagePreview>,
}

/// Bounded, single-line text displayed for a referenced message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagePreview {
    pub sender_name: Option<Box<str>>,
    pub text: Box<str>,
}

impl MessagePreview {
    pub fn from_text(sender_name: &str, text: &str, runs: &[TextRun]) -> Self {
        Self {
            sender_name: Some(Self::snippet(sender_name)),
            text: Self::message_snippet(text, runs),
        }
    }

    pub fn message_snippet(text: &str, runs: &[TextRun]) -> Box<str> {
        Self::snippet(&conceal_spoilers(text, runs))
    }

    pub fn snippet(text: &str) -> Box<str> {
        const MAX_CHARS: usize = 100;
        let mut snippet = String::new();
        let mut chars = text.trim().chars();
        for ch in chars.by_ref().take(MAX_CHARS) {
            snippet.push(if ch.is_whitespace() { ' ' } else { ch });
        }
        if chars.next().is_some() {
            snippet.push('…');
        }
        snippet.into_boxed_str()
    }
}

impl MessageReply {
    pub fn display_text(&self) -> &str {
        self.quote
            .as_deref()
            .or_else(|| self.preview.as_ref().map(|preview| preview.text.as_ref()))
            .unwrap_or("Loading original message…")
    }

    pub fn resolve(&mut self, preview: Option<MessagePreview>) {
        let mut preview = preview.unwrap_or_else(|| MessagePreview {
            sender_name: None,
            text: "Original message unavailable".into(),
        });
        if self.quote.is_some() {
            preview.text = Box::default();
        }
        self.preview = Some(preview);
    }
}

/// The small amount of chat data needed to render one main-list row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatSummary {
    pub id: i64,
    pub title: String,
    pub last_message_id: Option<i64>,
    pub last_message_date: Option<i32>,
    pub last_message_is_outgoing: bool,
    pub last_message_preview: String,
    pub unread_count: u32,
    pub last_read_inbox_message_id: i64,
    /// Read boundary in a private chat with another user. Zero means none read.
    pub last_read_outbox_message_id: Option<i64>,
    pub position: i64,
    pub can_receive_forwards: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_single_line_and_truncates_at_character_boundaries() {
        let text = format!("  one\ntwo {}  ", "я".repeat(101));
        let preview = MessagePreview::from_text(" Anna ", &text, &[]);

        assert_eq!(preview.sender_name.as_deref(), Some("Anna"));
        assert_eq!(preview.text.chars().count(), 101);
        assert!(preview.text.starts_with("one two "));
        assert!(preview.text.ends_with("я…"));
    }
}
