//! Conversions from TDLib updates into shared application events.

use tdlib_rs::{
    enums::{
        AuthorizationState as TdAuthorizationState, ChatList as TdChatList, MessageContent, Update,
    },
    types::Chat,
};

use crate::domain::{AuthorizationState, ChatMessage, ChatSummary};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum MessageSenderReference {
    Outgoing,
    User(i64),
    Chat(i64),
}

pub(super) fn chat(chat: &Chat) -> Option<ChatSummary> {
    let position = chat
        .positions
        .iter()
        .find(|position| position.list == TdChatList::Main)?;
    Some(ChatSummary {
        id: chat.id,
        title: chat.title.clone(),
        last_message_preview: chat.last_message.as_ref().map_or_else(
            || "No messages yet".into(),
            |message| match &message.content {
                MessageContent::MessageText(text) => text.text.text.clone(),
                _ => "Unsupported message".into(),
            },
        ),
        unread_count: chat.unread_count.max(0) as u32,
        position: position.order,
    })
}

pub(super) fn message_sender(message: &tdlib_rs::types::Message) -> MessageSenderReference {
    if message.is_outgoing {
        return MessageSenderReference::Outgoing;
    }
    match &message.sender_id {
        tdlib_rs::enums::MessageSender::User(sender) => {
            MessageSenderReference::User(sender.user_id)
        }
        tdlib_rs::enums::MessageSender::Chat(sender) => {
            MessageSenderReference::Chat(sender.chat_id)
        }
    }
}

pub(super) fn new_message_sender(update: &Update) -> Option<MessageSenderReference> {
    let Update::NewMessage(update) = update else {
        return None;
    };
    Some(message_sender(&update.message))
}

pub(super) fn new_message_id(update: &Update) -> Option<i64> {
    let Update::NewMessage(update) = update else {
        return None;
    };
    Some(update.message.id)
}

pub(super) fn new_message(
    update: &Update,
    sender_name: String,
    message_id: Option<i64>,
) -> Option<(i64, ChatMessage)> {
    let Update::NewMessage(update) = update else {
        return None;
    };
    let mut message = chat_message(update.message.clone(), sender_name);
    if let Some(message_id) = message_id {
        message.id = message_id;
    }
    Some((update.message.chat_id, message))
}

pub(super) fn input_text_content(text: String) -> tdlib_rs::enums::InputMessageContent {
    tdlib_rs::enums::InputMessageContent::InputMessageText(tdlib_rs::types::InputMessageText {
        text: tdlib_rs::types::FormattedText {
            text,
            entities: Vec::new(),
        },
        link_preview_options: None,
        clear_draft: true,
    })
}

pub(super) fn successful_message_sender(update: &Update) -> Option<MessageSenderReference> {
    let Update::MessageSendSucceeded(update) = update else {
        return None;
    };
    Some(message_sender(&update.message))
}

pub(super) fn successful_message_id(update: &Update) -> Option<i64> {
    let Update::MessageSendSucceeded(update) = update else {
        return None;
    };
    Some(update.old_message_id)
}

pub(super) fn message_send_succeeded(
    update: &Update,
    sender_name: String,
    client_message_id: Option<i64>,
) -> Option<(i64, i64, ChatMessage)> {
    let Update::MessageSendSucceeded(update) = update else {
        return None;
    };
    let mut message = chat_message(update.message.clone(), sender_name);
    if let Some(message_id) = client_message_id {
        message.id = message_id;
    }
    Some((update.message.chat_id, update.old_message_id, message))
}

pub(super) fn message_send_failed(update: &Update) -> Option<(i64, i64)> {
    let Update::MessageSendFailed(update) = update else {
        return None;
    };
    Some((update.message.chat_id, update.old_message_id))
}

pub(super) fn chat_message(message: tdlib_rs::types::Message, sender_name: String) -> ChatMessage {
    let text = match message.content {
        MessageContent::MessageText(text) => text.text.text,
        _ => "Unsupported message".into(),
    };
    ChatMessage {
        id: message.id,
        date: message.date,
        sender_name: if message.is_outgoing {
            "You".into()
        } else {
            sender_name
        },
        text,
        is_outgoing: message.is_outgoing,
    }
}

pub(super) fn user_name(user: &tdlib_rs::types::User) -> String {
    let name = format!("{} {}", user.first_name, user.last_name);
    let name = name.trim();
    if name.is_empty() {
        "Unknown sender".into()
    } else {
        name.into()
    }
}

pub(super) fn chat_title(chat: &Chat) -> String {
    if chat.title.is_empty() {
        "Unknown sender".into()
    } else {
        chat.title.clone()
    }
}

pub(super) fn chat_update_id(update: &Update) -> Option<i64> {
    match update {
        Update::NewMessage(update) => Some(update.message.chat_id),
        Update::NewChat(update) => Some(update.chat.id),
        Update::ChatTitle(update) => Some(update.chat_id),
        Update::ChatLastMessage(update) => Some(update.chat_id),
        Update::ChatPosition(update) => Some(update.chat_id),
        Update::ChatAddedToList(update) if update.chat_list == TdChatList::Main => {
            Some(update.chat_id)
        }
        Update::ChatRemovedFromList(update) if update.chat_list == TdChatList::Main => {
            Some(update.chat_id)
        }
        Update::ChatReadInbox(update) => Some(update.chat_id),
        _ => None,
    }
}

pub(super) fn chat_removed_from_main_list(update: &Update) -> Option<i64> {
    match update {
        Update::ChatRemovedFromList(update) if update.chat_list == TdChatList::Main => {
            Some(update.chat_id)
        }
        Update::ChatPosition(update)
            if update.position.list == TdChatList::Main && update.position.order == 0 =>
        {
            Some(update.chat_id)
        }
        Update::ChatLastMessage(update)
            if update
                .positions
                .iter()
                .any(|position| position.list == TdChatList::Main && position.order == 0) =>
        {
            Some(update.chat_id)
        }
        _ => None,
    }
}

pub(super) fn authorization_state(update: &Update) -> Option<AuthorizationState> {
    let Update::AuthorizationState(update) = update else {
        return None;
    };

    Some(authorization_state_from_tdlib(&update.authorization_state))
}

pub(super) fn authorization_state_from_tdlib(state: &TdAuthorizationState) -> AuthorizationState {
    match state {
        TdAuthorizationState::WaitTdlibParameters => AuthorizationState::WaitTdlibParameters,
        TdAuthorizationState::WaitPhoneNumber => AuthorizationState::WaitPhoneNumber,
        TdAuthorizationState::WaitCode(_) => AuthorizationState::WaitCode,
        TdAuthorizationState::WaitPassword(_) => AuthorizationState::WaitPassword,
        TdAuthorizationState::WaitRegistration(_) => AuthorizationState::WaitRegistration,
        TdAuthorizationState::Ready => AuthorizationState::Ready,
        TdAuthorizationState::Closed => AuthorizationState::Closed,
        _ => AuthorizationState::Other,
    }
}

/// Converts TDLib's error text to a fixed, non-sensitive message for the login UI.
pub(super) fn authorization_error(message: String) -> String {
    match message.as_str() {
        "PHONE_NUMBER_INVALID" => "Enter a valid phone number.".into(),
        "PHONE_NUMBER_BANNED" => "This phone number can't be used to sign in.".into(),
        "PHONE_CODE_INVALID" => "The login code is incorrect. Try again.".into(),
        "PHONE_CODE_EXPIRED" => "The login code expired. Request a new one.".into(),
        "PASSWORD_HASH_INVALID" => "The 2FA password is incorrect. Try again.".into(),
        "FLOOD_WAIT" | "FLOOD" => "Too many attempts. Wait a while and try again.".into(),
        _ => "Telegram couldn't complete sign-in. Check your details and try again.".into(),
    }
}
