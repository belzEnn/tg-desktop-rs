# 0002: Use iced's software renderer

- Status: accepted
- Date: 2026-11-11
- Decision owner: project owner

## Context

The application prioritizes low memory use and responsive idle behavior. The
renderer is selected based on the measurements in `docs/perf.md` and the
owner's explicit choice.

## Decision

Use iced's `tiny-skia` software renderer in the main application. The iced
dependency disables default features and explicitly enables `tiny-skia`,
`thread-pool`, `x11`, and `wayland`. WGPU is not enabled in the main crate.

## Evidence

The three-run empty-window data in `experiments/results.csv` gives these
medians:

| Renderer | RSS (KiB) | PSS (KiB) | Idle CPU |
| --- | ---: | ---: | ---: |
| tiny-skia | 18,580 | 11,628 | 0.02% |
| wgpu | 185,832 | 131,750 | 0.24% |

The owner's earlier estimates were about 18.5 MB / 3.3% for software and
190 MB / 6.1% for WGPU. Their CPU values came from the repeating frame
subscription probe and are superseded by the later one-shot measurements.

The synthetic software UI test further measured a 23,764 KiB median RSS at
UI-ready for 100 chat rows and 50 messages. Its median CPU tick count over a
10-second idle window was zero. This is a software-only workload and is not a
direct comparative test against WGPU. Details are in `docs/perf.md`.

## Consequences

- The regular application builds and runs with tiny-skia; the dependency graph
no longer includes WGPU.
- This choice prioritizes the measured memory and idle-CPU results. The
synthetic UI provides an initial workload check, but does not cover real
Telegram updates or all expected chat-list sizes.
- Revisit only if representative workloads reveal rendering or responsiveness
problems that outweigh the measured resource savings.
