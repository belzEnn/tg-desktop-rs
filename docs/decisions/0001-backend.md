# Backend decision: tdlib-rs

The project uses `tdlib-rs` as its Rust wrapper for TDLib, as selected by the
owner. Keep all TDLib types and conversions inside `src/tg`; application and UI
code consume only `src/domain` types.

The dependency is pinned to `1.4.0`. This release wraps TDLib `1.8.61`; the
`download-tdlib` feature supplies the native library for supported platforms.
The renderer decision is independent of this backend choice.

## Consequences

- TDLib's native library is downloaded during the build for supported targets.
- The TDLib receive call blocks, so updates are read on a dedicated thread.
- Telegram application credentials are read from `TG_API_ID` and
  `TG_API_HASH`; they are not stored in TOML or logged.
- Set `test_dc = true` in `config.toml` when using Telegram's test environment.
