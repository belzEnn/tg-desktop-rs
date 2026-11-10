A native Telegram client written in Rust. The primary goals are minimal RAM usage and fast responsiveness. 

## Tech Stack

- Rust stable, Tokio, iced (UI), tracing, thiserror and anyhow, serde and toml, dotenvy.
- Telegram backend: TDLib via tdlib-rs or Grammers. The choice will be made based on benchmarks during M0.5 and documented in `docs/decisions/`. Do not choose a backend or add its dependency before that decision is made.
- The iced renderer (wgpu or software renderer) will also be selected based on benchmarks.
- Dependency versions are pinned once and must not be changed without the owner's explicit approval.

## Commands

- `cargo run`
- `cargo test`
- `cargo clippy -- -D warnings`
- `cargo fmt`
- Memory measurement (Linux): `ps -o rss= -p <pid>` or `/usr/bin/time -v cargo run --release`

## Architecture

Three layers: `tg` (Telegram backend), `app` (application state and update logic), and `ui` (iced widgets). Shared types are defined in `domain`.

- Backend library types must only be visible inside `src/tg`. Only types from `src/domain` may be exposed externally. All conversions must be implemented in a single file: `src/tg/mapping.rs`.
- The UI must not communicate with the backend directly. It sends commands and receives events.
- Application state may only be modified in `update`. The `view` function is read-only.
- No blocking calls or disk/network operations are allowed in `update` or `view`.
- The backend must be replaceable without modifying `app` or `ui`.

## Code Rules

- No `unwrap` or `expect` outside tests and initialization in `main`. Errors must be typed (`thiserror`); `anyhow` is only allowed in `main`.
- Clippy must produce no warnings. `cargo fmt` must be applied.
- Public types must have a short comment explaining their purpose.
- Do not add dependencies without a good reason. Discuss every new dependency with the owner before adding it.
- Never invent APIs. If a signature is uncertain, inspect the crate source (`~/.cargo/registry`) or docs.rs and rely on verified documentation.

## Performance and Memory

- Every feature must be evaluated for its impact on RSS and response time. No media support in the MVP.
- Caches must have explicit size limits. Data from closed chats must not be retained in memory.
- The message list is limited to 50 messages in the MVP, with a button to load more. Virtualization will be implemented later.
- Avoid unnecessary cloning of large structures. Use references or `Arc` where appropriate.
- After each phase, record RSS in `docs/perf.md`: after login, after opening a chat, and after one hour of operation.

## Security

- `api_id`, `api_hash`, phone numbers, login codes, 2FA passwords, and message contents must never be logged at `info` level or above.
- `.env` and session data must never be committed to git.
- Use Telegram test servers during development. Do not automate actions that violate the Telegram API Terms of Service.
- Do not copy Telegram assets (icons, stickers, logos) into the repository.

## Development Process

1. Start by providing a plan of 5–10 lines and a list of files to be changed. Do not write code until the owner approves the plan.
2. One phase per branch, with small steps and commits (`feat`, `fix`, `refactor`, `docs`, `test`, `chore`).
3. After each step, run `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test`, then show the output.
4. After making changes, briefly explain what was done and why, so the owner can review it.
5. Do not change the tech stack, directory structure, or established decisions without explicit approval. If a change is necessary, propose it and wait for a response.
6. If a task exceeds the requested scope, stop and explain the issue instead of expanding the scope independently.