# ROADMAP

Nothing is built yet. This document plans the work from an empty repository to
an MVP and then toward a daily-driver client (Phases 6+). Slice
naming follows the repo convention (`Phase N: <short name> (#PR)`); one slice
per PR, each with tests and a DECISIONS.md entry.

Sources of truth, in order: `DECISIONS.md` per-slice specs (the "Out of this
slice" lists are the near-term backlog), this roadmap, `docs/spec.md`, and the
chosen backend's own schema or API docs (TDLib `td_api.tl` or the Telegram TL
schema for grammers — never invent constructors or methods; open the crate
source or docs.rs).

Status legend: ☐ planned · 🚧 in progress · ✅ done.

## Goals and memory budget

Primary goals: **low RSS** and **fast response**. The budget is fixed after the
first baseline (slice 0.2). Working hypothesis: RSS no higher than half of
Telegram Desktop on the same account and the same scenario.

Every slice records RSS in `docs/perf.md` (after login + chat list, after
opening a 50-message chat, after one idle hour from Phase 5). A slice that
pushes RSS over budget is not accepted until it is optimized or simplified.

Rough timeline (estimate, 15–20 h/week): MVP in about 6–7 weeks on TDLib, about
8–11 weeks on grammers (own cache, chat ordering and message storage).

## 0. Scaffold and measurements (keystone) ✅

Nothing else starts before the backend and renderer decisions are recorded.

- **0.1 Project skeleton.** ✅ Single crate with `domain`, `tg`, `app`, `ui`
  modules; config (`.env` + `config.toml`, data dir via `dirs`); `tracing`;
  `.gitignore` (target, `.env`, session data); `AGENTS.md` / `CLAUDE.md`;
  fmt, clippy and test checks; empty iced window. Exit: `cargo run` opens a
  window, all checks pass.
- **0.2 Backend and renderer measurements.** ✅ Minimal programs: login + load
  chat list on TDLib (tdlib-rs) and on grammers; empty iced window on wgpu and
  on the software renderer. Record RSS, time to first frame, time to chat list
  and integration effort in `docs/perf.md`.
- **0.3 Decision records.** ✅ Renderer decision recorded in
  `docs/decisions/0002-renderer.md`. Backend decision and
measurements.
  **Gate A**: after both choices are recorded, they are not revisited without
  a strong reason.

## 1. Backend connected and login

- **1.1 Backend adapter.** ✅ Client creation, receive loop on a dedicated
  thread, events through a tokio `mpsc` channel into an iced `Subscription`,
  commands as iced `Task`s; test-DC flag from config; backend log level
  lowered. Backend types live only in `src/tg`; conversion only in
  `src/tg/mapping.rs`. Exit: auth states are visible in the log.
- **1.2 Login flow.** ✅ Phone → code → 2FA screens driven by authorization
  events, not by button presses; inline errors; registration shows an
  "unsupported" message.
- **1.3 Session persistence and logout.** ✅ Restart without a new login;
  logout waits for the closed state and returns to the login screen.
  **Gate B**: if login does not work three weeks after the start, revisit the
  backend choice or the scope.

## 2. Chat list

- **2.1 Chat list display.** ☐ Main list sorted by position; title, last
  message preview (text or placeholder for unsupported content), unread
  counter; first-letter circle instead of avatars.
- **2.2 Live updates.** ☐ New message moves the chat to the top; unread and
  last-message changes apply without restart; connection-state banner.
- **2.3 Chat kinds.** ☐ Private, group and channel chats listed and labelled;
  composer hidden where posting is not possible.

## 3. Reading

- **3.1 Open chat and history.** ☐ Tell the backend which chat is open; load
  the last 50 messages; retry when the first response is short or empty;
  chronological order; unsupported content shown as a placeholder.
- **3.2 Load older messages.** ☐ "Load more" button (no infinite scroll yet).
- **3.3 Live incoming messages.** ☐ New messages appear in the open chat
  without reload.

## 4. Sending

- **4.1 Composer and text send.** ☐ Enter sends, Shift+Enter inserts a newline;
  temporary message id reconciled on the success update.
- **4.2 Send status and errors.** ☐ `Sending` / `Sent` / `Failed(reason)`
  shown per message; a failed send never breaks the chat.

## 5. Stabilization and MVP gate

- **5.1 Network loss and recovery.** ☐ Reconnect handling, banner, no panics
  while offline.
- **5.2 Clean shutdown.** ☐ Backend closed properly on window close.
- **5.3 MVP release.** ☐ README with build steps from a clean system,
  screenshots, known-issues list, final RSS measurements, tag `v0.1.0`.
  **Gate C**: continue or stop, recorded in `docs/decisions/0003-continue.md`.

## 6. Message essentials

- **6.1 Replies and forwarding.** ☐ Reply preview in rows; forward to another
  chat.
- **6.2 Edit and delete.** ☐ Own messages: edit and delete; live refresh from
  edit and delete updates.
- **6.3 Mark as read.** ☐ Mark messages read when viewed; unread counters stay
  consistent.
- **6.4 Text entities.** ☐ Bold, italic, underline, strikethrough, `code`,
  `pre` and links in message text; malformed spans dropped, unknown types
  ignored.

## 7. Scrolling at scale

- **7.1 Virtualized message list.** ☐ Only visible rows are built and drawn;
  stable scroll position when history loads above.
- **7.2 Infinite scroll upward.** ☐ Replaces the "Load more" button.
- **7.3 Lazy chat list.** ☐ Load chats in pages; keep only what is needed in
  memory.
- **7.4 Bounded caches.** ☐ Size limits and eviction for chats, users and
  messages; a script that checks RSS growth over a long session.

## 8. Media inside the budget

- **8.1 Avatars.** ☐ Small avatars with a bounded LRU cache.
- **8.2 Photos.** ☐ Thumbnails in rows, fullscreen viewer on click; nothing
  downloads automatically.
- **8.3 Files.** ☐ Document rows with explicit download; progress and cancel.

## 9.  Desktop integration

- **9.1 Notifications.** ☐ Pure notify/don't-notify decision (incoming,
  unmuted, newer than last read, app in background or chat not open); Linux
  first.
- **9.2 Search.** ☐ In-chat search and global search of chats and messages.
- **9.3 Chat folders.** ☐ Folder tabs filtering the chat list.
- **9.4 Links.** ☐ Open links and `tg://` links.

## 10. Collaboration (stretch)

- **10.1 Contributor setup.** ☐ `CONTRIBUTING.md`, issue and PR templates,
  module ownership (backend adapter, UI, media) so helpers can work in
  parallel instead of in rotation.

## Folded-in backlog

To be scheduled into the phases above or as small follow-ups as slices finish
and their "Out of this slice" lists grow: reactions; multi-pin and pinned
messages; draft persistence; custom mute durations; archive list; typing
indicators; per-user typing names in groups; message search filters; link
preview options on send; copy and quote-from-selection replies; hide-sender
forwards; emoji and sticker pickers; voice and audio playback; albums.

## Explicitly out of scope

Secret chats, voice and video calls, payments and Stars, multi-account,
telemetry, AI features, stories, bots and inline keyboards, Mini Apps,
distribution pipeline (packaging, signing, notarization). Windows and macOS
parity is not promised for the MVP.

## Blocked / needs verification

- Exact backend API signatures and compatible TDLib version — verify in the
  crate source and docs before use.
- Behaviour on a real account versus the Telegram test servers — verify on a
  secondary account, never the primary one.
- Windows and macOS builds — no test machines yet.
- Quality of the software renderer for a chat UI — decided by the Phase 0
  measurements.
