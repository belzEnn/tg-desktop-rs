# Phase 7.3 — Lazy dialog pages

Telegram Desktop maintains an ordered dialog list and requests more dialogs
as pages. This client loads 50 main-list summaries on sign-in and requests
another page as the sidebar approaches the lower edge. Forwarding uses the
same sidebar and pagination state. A failed request has an inline retry.

TDLib exposes a prefix of ranked IDs. Only the requested range of IDs is
resolved to summaries. The last ID in the page anchors the next request.
The next page can overlap a previous page when the ranking changes. The
application merges by ID and keeps live summaries for overlapping dialogs.
An update queue overflow schedules a cached snapshot refresh.

[Telegram Desktop dialog requests](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/apiwrap.cpp#L917).
