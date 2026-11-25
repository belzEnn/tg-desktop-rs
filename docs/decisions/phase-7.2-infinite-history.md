# Phase 7.2 — Infinite history

Telegram Desktop prefetches relative to the viewport height and suppresses
duplicate in-flight requests. This client fetches up to 50 earlier messages
when the upper edge is within one viewport. Short pages can still have older
messages. A page with no strictly older ID after one retry marks the end.
Requests carry an ID, time out, and run alongside the TDLib update loop.
The adapter cancels an older-page job when the user switches chats. An
upward gesture retries a failed request. The history keeps its message
anchor while earlier pages are inserted.

[Telegram Desktop history prefetch](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/history/history_widget.cpp#L5205).
