# Phase 6.4 — text entities

- Map TDLib text entities to checked UTF-8 byte ranges. Drop empty,
  out-of-range, surrogate-splitting, crossing, and unsupported spans while
  preserving their text. Combine supported styles in nested ranges.
- Render bold, italic, underline, and strikethrough with iced rich text.
  Inline code and preformatted spans use a monospaced font. Preformatted
  text retains indentation and line breaks. URL and text-link entities are
  underlined and colored. Link activation belongs to Phase 9.4.
- Mask spoiler text until a click reveals the message's hidden spans.
  Previews keep their masks. Changing chats or editing a message resets
  its reveal state. If a valid spoiler crosses a malformed formatting
  range, conceal the full message to keep the hidden text masked.
- Apply the same mapping to history, new messages, and content updates. An
  edit replaces the text and its runs together. Chat list and reply previews
  continue to display short plain-text snippets.

See [performance notes](../perf.md) for verification status.
