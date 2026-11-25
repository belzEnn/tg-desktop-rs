# Phase 7.4 — Bounded caches

Telegram Desktop can discard a loaded history and reload around a different
position. The application keeps a moving window of 250 messages for the open
chat, up to four targets of active actions, and the targets of up to 32
pending replies.
Pages contain up to 50 messages and can be fetched in either direction.
The viewport keeps its message anchor while the far edge is evicted.
Switching chats clears history and closes the previous chat in TDLib.
Sending from an older window reloads the latest page and keeps the pending
outgoing message visible.
[Telegram Desktop history implementation](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/history/history.cpp#L2958).

The main list keeps up to 150 summaries plus the active chat and the selected
forward destination. The window reloads cached IDs through TDLib when the
user scrolls back and preserves the first visible dialog across page changes.
The adapter resolves at most 100 distinct sender names in each temporary
page cache. It holds at most 32 pending sends for the open chat.

These limits cover Rust-owned summaries, history rows, and sender lookups.
TDLib manages its own cache and exposes offsets for history. Its main-list
snapshot is an ID prefix, which is temporary and grows with the requested
rank. [TDLib API schema](https://github.com/tdlib/td/blob/master/td/generate/scheme/td_api.tl)

The Linux RSS script samples peak growth relative to the first reading and
takes a growth limit in KiB. Its default duration is one hour.
