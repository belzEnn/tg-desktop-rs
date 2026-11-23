# Phase 6.3 — mark messages as read

- On redraw, use the actual scroll offset and visible history bounds. The
  newest incoming message whose bottom edge is visible sets the read boundary.
  A boundary also marks earlier messages read according to Telegram's inbox
  semantics.
- Send a read receipt only for the open chat in a focused window once its
  history is available. Keep at most one request in flight and coalesce new
  boundaries while it runs. Retry a rejected boundary on a fresh focus, on
  reopening the chat, or after a newer message becomes visible.
- Call TDLib `viewMessages` with `messageSourceChatHistory` and
  `force_read = false`. Use the unread count and inbox boundary provided by
  TDLib, and show the count in the chat list when it is nonzero.
- Use each private chat's outbox boundary from the TDLib snapshot and
  `updateChatReadOutbox`. Sent outgoing messages show one check, and messages
  at or before the read boundary show two. Pending messages and Saved
  Messages have no read check. Checks update without reopening the chat.

See [performance notes](../perf.md) for verification status.
