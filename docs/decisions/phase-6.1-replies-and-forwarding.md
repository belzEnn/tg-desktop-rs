# Phase 6.1 — Replies and forwarding

- Right-click a message to open the Reply and Forward actions at the cursor.
  Close the menu with Escape, a click outside, scrolling or a chat change.
  Iced's pinned release exposes the widget APIs for this through its
  `advanced` feature. Pointer motion does not dispatch application updates.
- Reply rows show the author and a short preview of the original message,
  preferring Telegram's quote when present. The composer shows the selected
  original and lets the user cancel the reply before sending.
- Resolve originals from the open history first, matching both chat and message
  identifiers. Fetch missing originals with TDLib `getRepliedMessage` in
  batches of at most four. A lookup has a three-second deadline. If the
  original is unavailable, show the quote or a placeholder.
- Send replies using `sendMessage` with `InputMessageReplyToMessage`.
  Reconcile temporary rows with the server's actual message identifier so a
  newly sent message can be referenced by either action.
- Forward one message at a time to a destination from the main chat list.
  Check `getMessageProperties` and then call `forwardMessages` with attribution.
  Wait for Telegram's send update when the operation returns a pending
  message. Display failures inline.
- Forum-topic selection, cross-chat reply composition, selecting a quote,
  navigating to the original, multi-message forwarding, and options to hide
  the original sender are outside this slice.

See [performance notes](../perf.md) for verification status.
