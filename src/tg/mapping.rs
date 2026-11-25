//! Conversions from TDLib updates into shared application events.

use std::{borrow::Cow, cmp::Reverse};

use tdlib_rs::{
    enums::{
        AuthorizationState as TdAuthorizationState, ChatList as TdChatList, MessageContent,
        MessageOrigin, MessageReplyTo, TextEntityType, Update,
    },
    types::{Chat, FormattedText, TextEntity},
};

use crate::domain::{
    AuthorizationState, BackendEvent, ChatMessage, ChatSummary, MessageActions, MessageAddress,
    MessagePreview, MessageReply, TextRun, TextStyle, conceal_spoilers,
};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub(super) enum MessageSenderReference {
    Outgoing,
    User(i64),
    Chat(i64),
}

pub(super) fn chat(chat: &Chat, account_user_id: Option<i64>) -> Option<ChatSummary> {
    let position = chat
        .positions
        .iter()
        .find(|position| position.list == TdChatList::Main)?;
    let last_read_outbox_message_id = match &chat.r#type {
        tdlib_rs::enums::ChatType::Private(peer)
            if account_user_id.is_some_and(|account| account != peer.user_id) =>
        {
            Some(chat.last_read_outbox_message_id)
        }
        _ => None,
    };
    let last_message = chat.last_message.as_ref();
    Some(ChatSummary {
        id: chat.id,
        title: chat.title.clone(),
        last_message_id: last_message.map(|message| message.id),
        last_message_date: last_message
            .map(|message| message.date)
            .filter(|&date| date > 0),
        last_message_is_outgoing: last_message.is_some_and(|message| message.is_outgoing),
        last_message_preview: last_message.map_or_else(
            || "No messages yet".into(),
            |message| MessagePreview::snippet(&preview_text(&message.content)).into(),
        ),
        unread_count: chat.unread_count.max(0) as u32,
        last_read_inbox_message_id: chat.last_read_inbox_message_id,
        last_read_outbox_message_id,
        position: position.order,
        can_receive_forwards: !matches!(chat.r#type, tdlib_rs::enums::ChatType::Secret(_)),
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

pub(super) fn origin_sender(origin: &MessageOrigin) -> Option<MessageSenderReference> {
    match origin {
        MessageOrigin::User(origin) => Some(MessageSenderReference::User(origin.sender_user_id)),
        MessageOrigin::Chat(origin) => Some(MessageSenderReference::Chat(origin.sender_chat_id)),
        MessageOrigin::Channel(origin) => Some(MessageSenderReference::Chat(origin.chat_id)),
        MessageOrigin::HiddenUser(_) => None,
    }
}

pub(super) fn input_reply(message_id: i64) -> tdlib_rs::enums::InputMessageReplyTo {
    tdlib_rs::enums::InputMessageReplyTo::Message(tdlib_rs::types::InputMessageReplyToMessage {
        message_id,
        quote: None,
        checklist_task_id: 0,
    })
}

pub(super) fn input_text_content(
    text: String,
    clear_draft: bool,
) -> tdlib_rs::enums::InputMessageContent {
    tdlib_rs::enums::InputMessageContent::InputMessageText(tdlib_rs::types::InputMessageText {
        text: tdlib_rs::types::FormattedText {
            text,
            entities: Vec::new(),
        },
        link_preview_options: None,
        clear_draft,
    })
}

fn formatted_preview(formatted: &FormattedText) -> Cow<'_, str> {
    if !formatted
        .entities
        .iter()
        .any(|entity| entity.r#type == TextEntityType::Spoiler)
    {
        return Cow::Borrowed(&formatted.text);
    }
    let runs = text_runs(&formatted.text, &formatted.entities);
    conceal_spoilers(&formatted.text, &runs)
}

fn preview_text(content: &MessageContent) -> Cow<'_, str> {
    match content {
        MessageContent::MessageText(message) => formatted_preview(&message.text),
        _ => Cow::Borrowed("Unsupported message"),
    }
}

pub(super) fn original_preview(
    message: &tdlib_rs::types::Message,
    sender_name: &str,
) -> MessagePreview {
    MessagePreview::from_text(sender_name, &preview_text(&message.content), &[])
}

fn message_reply(message: &mut tdlib_rs::types::Message) -> Option<Box<MessageReply>> {
    let MessageReplyTo::Message(reply) = message.reply_to.take()? else {
        return None;
    };
    let quote = reply
        .quote
        .map(|quote| MessagePreview::snippet(&formatted_preview(&quote.text)))
        .filter(|quote| !quote.is_empty());
    Some(Box::new(MessageReply {
        chat_id: reply.chat_id,
        message_id: reply.message_id,
        quote,
        preview: None,
    }))
}

pub(super) fn chat_message(
    mut message: tdlib_rs::types::Message,
    sender_name: String,
    origin_name: Option<String>,
) -> ChatMessage {
    let reply = message_reply(&mut message);
    let forwarded_from = message.forward_info.map(|forward| {
        let name = match &forward.origin {
            MessageOrigin::HiddenUser(origin) => &origin.sender_name,
            _ => origin_name.as_deref().unwrap_or("Unknown sender"),
        };
        MessagePreview::snippet(name)
    });
    let (text, text_runs, is_text) = display_content(message.content);
    ChatMessage {
        id: message.id,
        date: message.date,
        sender_name,
        text,
        text_runs,
        is_outgoing: message.is_outgoing,
        can_reference: message.sending_state.is_none(),
        is_text,
        is_edited: message.edit_date > 0,
        spoilers_revealed: false,
        reply,
        forwarded_from,
    }
}

fn display_content(content: MessageContent) -> (String, Box<[TextRun]>, bool) {
    match content {
        MessageContent::MessageText(message) => {
            let FormattedText { text, entities } = message.text;
            let runs = text_runs(&text, &entities);
            (text, runs, true)
        }
        _ => ("Unsupported message".into(), Box::default(), false),
    }
}

fn entity_style(kind: &TextEntityType) -> Option<TextStyle> {
    Some(match kind {
        TextEntityType::Spoiler => TextStyle::SPOILER,
        TextEntityType::Bold => TextStyle::BOLD,
        TextEntityType::Italic => TextStyle::ITALIC,
        TextEntityType::Underline => TextStyle::UNDERLINE,
        TextEntityType::Strikethrough => TextStyle::STRIKETHROUGH,
        TextEntityType::Code => TextStyle::CODE,
        TextEntityType::Pre | TextEntityType::PreCode(_) => TextStyle::PRE,
        TextEntityType::Url | TextEntityType::TextUrl(_) => TextStyle::LINK,
        _ => return None,
    })
}

fn text_runs(text: &str, entities: &[TextEntity]) -> Box<[TextRun]> {
    if entities.is_empty() {
        return Box::default();
    }
    let ascii = text.is_ascii();
    let mut boundaries = None;
    let mut spans: Vec<_> = entities
        .iter()
        .filter_map(|entity| {
            let style = entity_style(&entity.r#type)?;
            let start = usize::try_from(entity.offset).ok()?;
            let length = usize::try_from(entity.length).ok()?;
            let end = start.checked_add(length)?;
            if start == end || end > text.len() {
                return None;
            }
            if ascii {
                return Some((start, end, style));
            }
            let boundaries = boundaries.get_or_insert_with(|| utf16_boundaries(text));
            let start = *boundaries.get(start)?;
            let end = *boundaries.get(end)?;
            (start != usize::MAX && end != usize::MAX).then_some((start, end, style))
        })
        .collect();
    if spans.is_empty() {
        return Box::default();
    }

    // Visit enclosing spans first. A span that crosses an enclosing span's end
    // cannot be nested and is discarded.
    spans.sort_unstable_by_key(|&(start, end, _)| (start, Reverse(end)));
    let mut active: Vec<TextRun> = Vec::new();
    let mut runs = Vec::new();
    for (start, end, style) in spans {
        while let Some(finished) = active.last().filter(|run| run.end <= start) {
            append_run(&mut runs, finished.end, finished.style);
            active.pop();
        }
        if active.last().is_some_and(|parent| end > parent.end) {
            if style == TextStyle::SPOILER {
                // A broken style range must not expose known hidden text.
                return Box::new([TextRun {
                    end: text.len(),
                    style: TextStyle::SPOILER,
                }]);
            }
            continue;
        }
        let outer = active
            .last()
            .map_or_else(TextStyle::default, |run| run.style);
        append_run(&mut runs, start, outer);
        active.push(TextRun {
            end,
            style: outer.union(style),
        });
    }
    for run in active.into_iter().rev() {
        append_run(&mut runs, run.end, run.style);
    }
    append_run(&mut runs, text.len(), TextStyle::default());
    runs.into_boxed_slice()
}

fn utf16_boundaries(text: &str) -> Vec<usize> {
    let mut boundaries = Vec::new();
    for (index, ch) in text.char_indices() {
        boundaries.push(index);
        if ch.len_utf16() == 2 {
            // This UTF-16 offset splits a surrogate pair.
            boundaries.push(usize::MAX);
        }
    }
    boundaries.push(text.len());
    boundaries
}

fn append_run(runs: &mut Vec<TextRun>, end: usize, style: TextStyle) {
    if end <= runs.last().map_or(0, |run| run.end) {
        return;
    }
    if let Some(last) = runs.last_mut().filter(|run| run.style == style) {
        last.end = end;
    } else {
        runs.push(TextRun { end, style });
    }
}

pub(super) fn message_actions(properties: tdlib_rs::enums::MessageProperties) -> MessageActions {
    let tdlib_rs::enums::MessageProperties::MessageProperties(properties) = properties;
    MessageActions {
        can_edit: properties.can_be_edited,
        can_delete_for_self: properties.can_be_deleted_only_for_self,
        can_delete_for_everyone: properties.can_be_deleted_for_all_users,
    }
}

pub(super) fn message_change(update: Update) -> Option<BackendEvent> {
    match update {
        Update::ChatReadInbox(update) => Some(BackendEvent::ChatInboxRead {
            chat_id: update.chat_id,
            last_read_message_id: update.last_read_inbox_message_id,
            unread_count: update.unread_count.max(0) as u32,
        }),
        Update::ChatReadOutbox(update) => Some(BackendEvent::ChatOutboxRead {
            chat_id: update.chat_id,
            last_read_message_id: update.last_read_outbox_message_id,
        }),
        Update::MessageContent(update) => {
            let (text, text_runs, is_text) = display_content(update.new_content);
            Some(BackendEvent::MessageContentChanged {
                address: MessageAddress {
                    chat_id: update.chat_id,
                    message_id: update.message_id,
                },
                text,
                text_runs,
                is_text,
            })
        }
        Update::MessageEdited(update) if update.edit_date > 0 => {
            Some(BackendEvent::MessageEdited(MessageAddress {
                chat_id: update.chat_id,
                message_id: update.message_id,
            }))
        }
        Update::DeleteMessages(update) if !update.from_cache => {
            Some(BackendEvent::MessagesDeleted {
                chat_id: update.chat_id,
                message_ids: update.message_ids,
            })
        }
        _ => None,
    }
}

pub(super) fn user_id(user: tdlib_rs::enums::User) -> i64 {
    let tdlib_rs::enums::User::User(user) = user;
    user.id
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

#[cfg(test)]
mod tests {
    use super::*;
    use tdlib_rs::types::UpdateDeleteMessages;

    #[test]
    fn cache_eviction_is_not_a_message_deletion() {
        let update = UpdateDeleteMessages {
            chat_id: 10,
            message_ids: vec![2],
            is_permanent: false,
            from_cache: true,
        };
        assert!(message_change(Update::DeleteMessages(update.clone())).is_none());
        assert_eq!(
            message_change(Update::DeleteMessages(UpdateDeleteMessages {
                from_cache: false,
                ..update
            })),
            Some(BackendEvent::MessagesDeleted {
                chat_id: 10,
                message_ids: vec![2]
            }),
        );
    }

    fn entity(offset: i32, length: i32, kind: TextEntityType) -> TextEntity {
        TextEntity {
            offset,
            length,
            r#type: kind,
        }
    }

    #[test]
    fn utf16_offsets_keep_whole_unicode_scalars_and_drop_invalid_entities() {
        let text = "a🙂бz";
        let entities = [
            entity(3, 1, TextEntityType::Bold),
            entity(1, 2, TextEntityType::Underline),
            entity(2, 1, TextEntityType::Italic), // Starts inside the emoji.
            entity(0, 2, TextEntityType::Strikethrough), // Ends inside the emoji.
            entity(-1, 3, TextEntityType::Italic),
            entity(0, 0, TextEntityType::Url),
            entity(4, 2, TextEntityType::Code), // Beyond the UTF-16 length.
            entity(4, 1, TextEntityType::Mention), // Unsupported type.
        ];

        assert_eq!(
            &*text_runs(text, &entities),
            &[
                TextRun {
                    end: 1,
                    style: TextStyle::default()
                },
                TextRun {
                    end: "a🙂".len(),
                    style: TextStyle::UNDERLINE
                },
                TextRun {
                    end: "a🙂б".len(),
                    style: TextStyle::BOLD
                },
                TextRun {
                    end: text.len(),
                    style: TextStyle::default()
                },
            ]
        );
        assert!(text_runs(text, &entities[2..]).is_empty());
    }

    #[test]
    fn nested_entities_combine_and_a_crossing_range_is_ignored() {
        let entities = [
            entity(2, 2, TextEntityType::Italic),
            entity(1, 5, TextEntityType::Bold),
            entity(3, 4, TextEntityType::Strikethrough), // Crosses the enclosing spans.
        ];
        assert_eq!(
            &*text_runs("abcdefgh", &entities),
            &[
                TextRun {
                    end: 1,
                    style: TextStyle::default()
                },
                TextRun {
                    end: 2,
                    style: TextStyle::BOLD
                },
                TextRun {
                    end: 4,
                    style: TextStyle::BOLD.union(TextStyle::ITALIC)
                },
                TextRun {
                    end: 6,
                    style: TextStyle::BOLD
                },
                TextRun {
                    end: 8,
                    style: TextStyle::default()
                },
            ]
        );
    }

    #[test]
    fn tdlib_content_update_carries_preformatted_text_and_link_runs() {
        use tdlib_rs::types::{
            MessageText, TextEntityTypePreCode, TextEntityTypeTextUrl, UpdateMessageContent,
        };

        let text = "a\n  b link";
        let update = Update::MessageContent(UpdateMessageContent {
            chat_id: 17,
            message_id: 42,
            new_content: MessageContent::MessageText(MessageText {
                text: FormattedText {
                    text: text.into(),
                    entities: vec![
                        entity(
                            0,
                            5,
                            TextEntityType::PreCode(TextEntityTypePreCode {
                                language: "rust".into(),
                            }),
                        ),
                        entity(
                            6,
                            4,
                            TextEntityType::TextUrl(TextEntityTypeTextUrl {
                                url: "https://example.com".into(),
                            }),
                        ),
                    ],
                },
                link_preview: None,
                link_preview_options: None,
            }),
        });
        assert_eq!(
            message_change(update),
            Some(BackendEvent::MessageContentChanged {
                address: MessageAddress {
                    chat_id: 17,
                    message_id: 42
                },
                text: text.into(),
                text_runs: Box::new([
                    TextRun {
                        end: 5,
                        style: TextStyle::PRE
                    },
                    TextRun {
                        end: 6,
                        style: TextStyle::default()
                    },
                    TextRun {
                        end: 10,
                        style: TextStyle::LINK
                    },
                ]),
                is_text: true,
            })
        );
    }

    #[test]
    fn formatted_preview_masks_spoilers_and_preserves_visible_unicode() {
        let formatted = FormattedText {
            text: "я 🙂x конец".into(),
            entities: vec![
                entity(2, 3, TextEntityType::Spoiler),
                entity(4, 1, TextEntityType::Bold),
            ],
        };
        assert_eq!(formatted_preview(&formatted), "я •• конец");
    }

    #[test]
    fn crossing_formatting_does_not_expose_a_spoiler_in_a_preview() {
        let formatted = FormattedText {
            text: "abcdef".into(),
            entities: vec![
                entity(0, 4, TextEntityType::Bold),
                entity(2, 4, TextEntityType::Spoiler),
            ],
        };
        assert_eq!(formatted_preview(&formatted), "••••••");
    }
}
