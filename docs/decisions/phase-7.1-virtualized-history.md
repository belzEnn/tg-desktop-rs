# Phase 7.1 — Virtualized history

Telegram Desktop preserves a message anchor when refreshing a history slice
and restricts painting to the relevant part of the history. The iced client
keeps the native scrollable and builds variable-height widgets for rows that
intersect the viewport. Spacers represent rows outside the viewport. The
widget measures rows at the current column width and estimates the heights
of unmeasured rows. It retains up to 1,024 measured heights for the open chat
and clears them when the chat or column width changes.

Prepending history preserves the first visible message and the pixel offset
inside it. At the newest edge the viewport remains pinned to the last row.
Read receipts use the bounds of the rendered messages.

[Telegram Desktop history layout and scroll anchor](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/history/history_inner_widget.cpp#L5637).
