// Application state and update logic.

use iced::{Subscription, Task};

use crate::{
    domain::{AuthorizationState, BackendEvent},
    tg::{Adapter, AdapterCommandError},
};

/// State owned by the application's update loop.
pub(crate) struct State {
    backend: Adapter,
    authorization_state: Option<AuthorizationState>,
}

/// Messages handled by the application update loop.
#[derive(Debug, Clone)]
pub(crate) enum Message {
    BackendEvent(BackendEvent),
    Initialized(Result<(), AdapterCommandError>),
}

/// Creates application state and starts the backend through an iced task.
pub(crate) fn boot(backend: Adapter) -> (State, Task<Message>) {
    let state = State {
        backend: backend.clone(),
        authorization_state: None,
    };
    let task = Task::perform(
        async move { backend.initialize().await },
        Message::Initialized,
    );
    (state, task)
}

/// Subscribes the application to backend events.
pub(crate) fn subscription(state: &State) -> Subscription<Message> {
    state.backend.subscription().map(Message::BackendEvent)
}

/// Applies backend events and command results to application state.
pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::BackendEvent(BackendEvent::AuthorizationChanged(authorization_state)) => {
            state.authorization_state = Some(authorization_state);
            tracing::info!(
                state = authorization_state.as_str(),
                "Telegram authorization state changed"
            );
        }
        Message::Initialized(Ok(())) => {}
        Message::Initialized(Err(error)) => {
            tracing::error!(%error, "Telegram backend initialization command failed");
        }
    }
    Task::none()
}

impl State {
    pub(crate) fn authorization_status(&self) -> &'static str {
        self.authorization_state
            .map(AuthorizationState::as_str)
            .unwrap_or("starting")
    }
}
