# Performance measurements

## Synthetic software-renderer UI

Three release runs of the iced/tiny-skia synthetic chat screen. The screen has
100 scrollable chat rows, 50 text messages in the selected chat, and a composer.
It uses no Telegram connection, real account data, media, or live updates.

| Metric | Median | Raw runs |
| --- | ---: | --- |
| First frame | 76.08 ms | 76.83 / 74.28 / 76.08 ms |
| RSS immediately after process creation (`VmRSS`) | 28 KiB* | 104 / 28 / 24 KiB |
| RSS at UI ready (`VmRSS`) | 23,764 KiB | 23,764 / 23,712 / 23,852 KiB |
| RSS after idle interval (`VmRSS`) | 24,012 KiB | 29,660 / 23,936 / 24,012 KiB |
| CPU during idle interval | 0.00% | 0.30 / 0.00 / 0.00% |

\* The launch RSS sample is taken immediately after spawning the process, before
the executable has finished loading. Its 24–104 KiB values are not a useful
representation of application memory and should not be compared with a running
window. Use the UI-ready and idle RSS values for comparisons.

CPU is computed from process CPU-time ticks over each 10-second idle interval.
The observed zero values mean no CPU tick was recorded in those intervals; they
do not establish a finer-grained 0.00% load. Three runs were performed, with
the median reported above. The raw values are in
[`experiments/results-soft-synthetic.csv`](../experiments/results-soft-synthetic.csv).

### Measurement method

- Release executable built with `cargo +stable build --manifest-path
  experiments/render-soft/Cargo.toml --release`.
- Ran `python3 experiments/measure_all.py --runs 3 --interaction 15 --idle 10`.
- RSS is `VmRSS` from `/proc/<pid>/status`, recorded after process creation,
  when the program reports `UI_READY=chats:100,messages:50`, and after the idle
  interval.
- First-frame time starts at the first instruction in `main` and ends when
  iced delivers its first `window::frames()` event. It is a first-frame event,
  not a compositor presentation timestamp.
- Each run allowed 15 seconds for manual scrolling, followed by 10 seconds
  without interaction for CPU and idle RSS sampling.
- Median: middle value of the three runs. CPU values are per-run averages over
  the idle interval, then medianed.

### Environment

| Item | Value |
| --- | --- |
| Date | 2026-11-11 |
| OS | Arch Linux rolling, x86_64 |
| Kernel | Linux 7.2.14-arch1-1 |
| CPU | AMD Ryzen 5 6600H with Radeon Graphics, 6 cores / 12 threads |
| Memory | 14 GiB visible to the measurement process |
| Rust | `rustc 1.98.0 (88d9e12ae 2026-08-18)` stable |
| iced | `0.14.0` |
| Renderer | `tiny-skia` (`iced_tiny_skia 0.14.1`) |

The experiment verifies a moderately populated UI on the software renderer.
It does not establish performance with real Telegram data or continuous live
updates. Backend measurements and a WGPU comparison are not recorded here.

## Backend adapter (Phase 1.1)

RSS and response-time measurements for an authenticated TDLib session have not
been collected yet. The adapter introduces one TDLib receive thread, one small
Tokio supervisor runtime, and bounded channels (64 queued updates and events;
8 queued commands). Record the required after-login, open-chat, and idle-hour
measurements when the adapter is run against a test account.

## Login flow (Phase 1.2)

The login UI adds three small input strings and one inline-error string to
application state. The password field is masked and its stored value is
cleared after submission and on authorization changes. Authorization errors
are converted to fixed display messages, so TDLib error text is not retained.
No media, message history, or unbounded cache is added by this slice.

RSS measurements after login, opening a chat, and one idle hour remain pending.
Measure these scenarios against the Telegram test server with a test account.

## Session persistence and logout (Phase 1.3)

TDLib's database directory remains under the configured application data
directory, allowing a session to resume after a restart. Logout uses TDLib's
proper `logOut` request, which removes local session data and closes the client;
the application waits for the resulting `Closed` authorization state before it
starts a fresh TDLib client and returns to phone authorization. This adds no
in-memory cache. RSS measurements on an authenticated session remain pending.

## Chat list display (Phase 2.1)

The main chat list retains at most 50 lightweight summaries in application
state. TDLib loads at most 50 chats for the initial display; no avatars or
media are downloaded. RSS measurements after login, opening a chat, and one
idle hour remain pending until the slice is run with a Telegram test account.

## Open chat and history (Phase 3.1)

Opening a chat tells TDLib which chat is active and requests history in bounded
pages until up to 50 messages are available. Application state retains only
the selected chat's messages; switching chats clears the previous history.
RSS measurements after login, opening a 50-message chat, and one idle hour
remain pending until measured with a Telegram test account.

## Load older messages (Phase 3.2)

The open chat begins with at most 50 messages and requests another page of 50
when scrolling reaches approximately the 30th message from newest. Text history
is retained only for the selected chat and cleared when another chat is opened;
there is no separate history cache. Memory use can therefore grow with the
number of pages visited in the active chat. RSS measurements after login,
opening a chat, and one idle hour remain pending with a Telegram test account.

## Phase 6.1 — replies and forwarding

Reply previews reuse open history before querying TDLib. Each preview field
is limited to 100 Unicode characters plus an ellipsis. At most four original
messages are fetched concurrently, and the first history page renders before
these lookups finish. The UI releases saved previews when the chat changes.
The destination picker reuses the bounded main chat list.

RSS after login, after opening a 50-message chat, and after one idle hour
remains pending, along with reply-preview latency and an end-to-end forwarding
check. This environment could not start the GUI on Wayland or bind an Xvfb
display, so no authenticated Telegram test-DC session was exercised.

## Phase 6.2 — edit and delete messages

Opening an outgoing message's menu requests its properties once. It adds no
per-message permission cache or history fetch. Mutations run off the backend
command loop, with one mutation tracked by the UI at a time. Editing holds
one existing draft while reusing the composer. Live changes update the loaded
rows and related previews in place.

RSS after login, after opening a 50-message chat, and after one idle hour
remains pending, along with edit and delete latency and an end-to-end check
against Telegram's test DC.

## Phase 6.3 — mark messages as read

The visible history uses existing layout bounds and a binary search to find
the bottommost eligible row. It allocates no per-message visibility cache.
Read boundaries are coalesced into one request at a time. Inbox updates
apply the count directly without querying the chat again. Outgoing checks
use one boundary per listed private chat. TDLib provides the account user
ID at sign-in so Saved Messages can be excluded.

RSS after login, after opening a 50-message chat, and after one idle hour
remains pending, along with the delay from viewing a message to the unread
count changing on Telegram's test DC.

## Phase 6.4 — text entities

Each formatted row stores byte boundaries and combined style flags beside
its existing text. Plain rows allocate no formatting runs and use the plain
text widget. Styled fragments borrow the original text. UTF-16 conversion
and span sorting run when TDLib content arrives. The conversion table is
temporary. Spoiler reveal state lives on the loaded message row. The UI
builds masked fragments only for concealed spoilers.

RSS after login, after opening a 50-message chat, and after one idle hour
remains pending, along with rendering and live edit verification on
Telegram's test DC.

## Phase 6.5 — UI Foundation

Five paired synthetic runs compared the production views at `97d68fe` and
Phase 6.5, with 50 dialogs and 50 text messages, a 1024 × 768 viewport, and
the tiny-skia renderer. Each run used a fresh debug test process on the same
machine. After one warm-up, the probe rebuilt the view and recorded draw
primitives 50 times using the same widget tree. It then rasterized one
frame and read Linux `VmRSS`.

| Metric | Before | Phase 6.5 |
| --- | ---: | ---: |
| Median layout and draw preparation per iteration | 0.652 ms | 0.773 ms |
| Median process RSS after one rasterized frame | 18,692 KiB | 18,928 KiB |
| Per-run preparation averages (ms) | 0.652 / 0.660 / 0.650 / 0.641 / 0.655 | 0.770 / 0.795 / 0.773 / 0.781 / 0.768 |
| Per-run RSS (KiB) | 18,784 / 18,692 / 18,664 / 18,676 / 18,916 | 19,176 / 18,888 / 18,928 / 18,768 / 19,224 |

The new view added 0.121 ms and 236 KiB to these medians. These debug,
offscreen measurements exclude GPU or compositor presentation, the Telegram
worker, idle behavior, and interactive input. The earlier release window
probe measured a different workload.

The message rows borrow the open history and introduce no media buffers or
row cache. They build rows for the loaded history and use widget layout
bounds for read tracking. The UI adds no recurring subscription. A display
connection and an authenticated test session were unavailable. RSS after
login, opening a 50-message chat, and one idle hour, plus scroll and resize
latency, remain pending.
