// Telegram backend adapter and backend-specific implementation.

mod adapter;
mod mapping;

pub(crate) use adapter::{Adapter, AdapterCommandError};
