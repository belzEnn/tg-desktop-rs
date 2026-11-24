# UI visual baseline

The fixture renders the production view with iced 0.14 and tiny-skia using an
in-memory adapter. The five dialogs and nine messages are fixed. They exercise
the active dialog, unread counts, a long title, multiline text, formatted text,
a reply, a forwarded placeholder, an edit label, outgoing read receipts,
and a run of consecutive short messages.

| Setting | Baseline |
| --- | --- |
| Content viewport | 680 × 480 px, 1024 × 768 px, and 1920 × 1024 px |
| Scale | 1.0 (100%, with one physical pixel per logical pixel) |
| Theme | Light and dark |
| Font | iced default sans-serif and the system font configuration |
| History position | At the latest message |
| Timestamp clock | UTC |
| Pointer | Outside the view |

## Capture the fixture

Build with the project's usual local TDLib installation. Run this command in the
repository root.

```sh
UI_BASELINE_DIR=/tmp/tg-desktop-rs-baseline \
  cargo test ui::visual::capture_visual_baseline -- --ignored --exact
```

The test writes six PPM files outside Git. ImageMagick can convert them to PNG.

```sh
for shot in /tmp/tg-desktop-rs-baseline/*.ppm; do
  magick "$shot" "${shot%.ppm}.png"
done
```

For a subsequent change, use separate output directories and the same toolchain,
fonts, and renderer. Compare each matching screenshot visually or generate a
difference image with `magick compare before.ppm after.ppm diff.png`. The regular
test suite checks that the sidebar, header, history, and composer fit at the
minimum, default, and 1920 × 1024 window sizes. Screenshot generation is opt-in.
Pixel comparisons have no automated pass or fail threshold.

## Compare with Telegram Desktop

Use a test account with synthetic conversations in Telegram Desktop. Capture
the same messages at 100% scale in its light and dark themes. Match the content
viewport dimensions and latest-message position, and crop window decorations.
Record the Telegram Desktop version, OS, font configuration, and theme with the
captures. Compare panel proportions, spacing, text hierarchy, dialogue states,
bubble alignment, and the composer side by side. Qt text rendering, system font
fallback, native decorations, and wallpaper affect the raster result. This
phase used the following sources as references for those layout and style roles.

- [Window and column geometry](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/window/window.style)
- [Dialog metrics and styles](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/dialogs/dialogs.style)
- [Dialog row states](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/dialogs/ui/dialogs_layout.style)
- [History header and composer geometry](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/history/history_widget.cpp)
- [Message layout and content](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/history/view/history_view_message.cpp)
- [Embedded Day and Night themes](https://github.com/telegramdesktop/tdesktop/blob/dev/Telegram/SourceFiles/window/themes/window_themes_embedded.cpp)

The latest pass compares these synthetic captures with the supplied light and
dark screenshots at desktop size. The compact rows use inline metadata when
it fits. The short history ends above the composer. The history fills the
conversation pane, and each message bubble has a maximum width. Token values
are specific to this client. The message width cap and padding follow the
reference chat style. This client uses 15 logical pixels for the message
body and 13 for timestamps.
The captured baseline covers the client's offscreen output. A side-by-side run with Telegram Desktop and live
input, resizing, and test-DC flows remain to be verified.

## Extending message rendering

The message component chooses a borrowed text or placeholder view, then applies
the shared bubble, reply and forward details, metadata, and context-menu
interaction. Future photos, GIFs, videos, documents, and stickers can add
content variants when the domain exposes the required data. The history owns
pagination and scroll geometry. It can later build visible rows on demand
using the same row renderer and stable domain IDs. Current rows have variable
height and the history draws all loaded rows that intersect the viewport.
