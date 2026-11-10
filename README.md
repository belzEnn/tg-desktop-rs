# Telegram-desktop on Rust

An unofficial, early-stage Telegram desktop client written in Rust. The goals
are low memory use and a fast, responsive interface, with a small, focused
feature set rather than parity with Telegram Desktop.

> **Status:** pre-alpha. Nothing is released yet. See [ROADMAP.md](ROADMAP.md)
> for the plan and [docs/spec.md](docs/spec.md) for the specification.

## Goals

- Low RSS and quick startup (measured and tracked in `docs/perf.md`)
- Native UI with iced, no browser engine or web view
- A clean boundary between the Telegram backend and the interface

## Disclaimer

tg-iced is an independent project. It is not affiliated with, endorsed by, or
sponsored by Telegram. You need your own API credentials from
https://my.telegram.org. Please follow the Telegram API Terms of Service; the
use of third-party clients can lead to limits on an account, so test with a
secondary account or the Telegram test servers.