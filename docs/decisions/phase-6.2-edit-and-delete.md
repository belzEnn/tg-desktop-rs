# Phase 6.2 — edit and delete messages

- The context menu requests TDLib message properties for the selected
  outgoing message and shows the allowed actions. Text editing uses the chat
  composer with Save and Cancel. The existing draft and reply selection are
  restored after saving or canceling.
- Editing submits plain text through `editMessageText`. Deleting a message
  requires confirmation and an explicit choice of a permitted audience.
  `deleteMessages` receives that choice as its `revoke` argument.
- `updateMessageContent` replaces the displayed content and refreshes ordinary
  reply previews and the chat list preview when it refers to the same message.
  `updateMessageEdited` sets the edited label. `updateDeleteMessages` removes
  inaccessible messages and marks their originals unavailable in reply
  previews. Explicit quotes remain visible and cache eviction leaves rows
  intact.
- Bulk deletion, moderation of other users' messages, media captions, and
  formatting-aware editing are outside this slice.

See [performance notes](../perf.md) for verification status.
