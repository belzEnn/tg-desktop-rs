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
