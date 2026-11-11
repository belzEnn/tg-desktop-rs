# Phase 3.1: Open chat and history

## Scope

- The UI selects a chat and the application tells TDLib to open it.
- TDLib history is fetched in bounded requests until 50 messages are available
  or the chat has no more history. An empty first response is retried once.
- Only messages belonging to the currently selected chat are retained by the
  application. Stale responses for previously selected chats are ignored.
- Messages are displayed chronologically. Text is shown directly and other
  content receives an `Unsupported message` placeholder.

## TDLib API

The implementation uses `tdlib-rs = 1.4.0` generated signatures for
`open_chat(chat_id, client_id)` and
`get_chat_history(chat_id, from_message_id, offset, limit, only_local,
client_id)`. The schema documents reverse chronological results and permits
fewer results than requested, so history is requested from the oldest received
message until the cap is met or no progress is possible.

## Out of this slice

Loading older messages on demand, live incoming-message refresh, read receipts,
message sender names, media rendering, and message virtualization.

## Measurements

RSS for login, opening a chat, and one idle hour remains pending; see
[`docs/perf.md`](../perf.md).
