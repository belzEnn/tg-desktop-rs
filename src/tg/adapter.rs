//! TDLib client lifecycle and bounded event delivery.

use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

use iced::{Subscription, futures::SinkExt, stream};
use thiserror::Error;
use tokio::{
    sync::{Mutex, mpsc},
    task::JoinSet,
};
use tracing::{error, info, warn};

use crate::domain::{
    AuthorizationState, BackendEvent, ChatMessage, ChatPageQuery, DeleteScope, MessageAction,
    MessageAddress,
};

use super::mapping;

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);
const EVENT_CHANNEL_CAPACITY: usize = 64;
const UPDATE_CHANNEL_CAPACITY: usize = 64;
const COMMAND_CHANNEL_CAPACITY: usize = 8;
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);
const CHAT_PAGE_TIMEOUT: Duration = Duration::from_secs(15);
const CHAT_HISTORY_LIMIT: usize = 50;
const HISTORY_TIMEOUT: Duration = Duration::from_secs(15);
const REPLY_LOOKUP_TIMEOUT: Duration = Duration::from_secs(3);

/// Errors raised while starting the TDLib adapter.
#[derive(Debug, Error)]
pub(crate) enum AdapterError {
    #[error("could not start TDLib worker thread: {0}")]
    Thread(#[from] std::io::Error),
    #[error("could not create TDLib worker runtime: {0}")]
    Runtime(std::io::Error),
}

/// Errors returned when sending a command to the backend worker.
#[derive(Debug, Clone, Copy, Error)]
pub(crate) enum AdapterCommandError {
    #[error("TDLib worker has stopped")]
    WorkerStopped,
}

/// Handle used by the application to control TDLib and subscribe to its events.
#[derive(Clone)]
pub(crate) struct Adapter {
    commands: mpsc::Sender<BackendCommand>,
    events: SubscriptionSource,
}

enum BackendCommand {
    Initialize,
    LogOut,
    SubmitPhone(String),
    SubmitCode(String),
    SubmitPassword(String),
    OpenChat(i64),
    LoadChatPage(ChatPageQuery),
    MarkRead {
        address: MessageAddress,
        request_id: u64,
    },
    LoadOlderMessages {
        chat_id: i64,
        before_message_id: i64,
        request_id: u64,
    },
    SendText {
        chat_id: i64,
        message_id: i64,
        text: String,
        reply_to: Option<i64>,
    },
    ResolveReplies {
        chat_id: i64,
        message_ids: Vec<i64>,
    },
    MessageAction {
        address: MessageAddress,
        request_id: u64,
        action: MessageAction,
    },
    Forward {
        from_chat_id: i64,
        message_id: i64,
        to_chat_id: i64,
    },
}

struct SubscriptionSource {
    id: u64,
    receiver: Arc<Mutex<mpsc::Receiver<BackendEvent>>>,
}

impl Clone for SubscriptionSource {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            receiver: Arc::clone(&self.receiver),
        }
    }
}

impl Hash for SubscriptionSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Adapter {
    /// Creates a TDLib client and starts its dedicated update receiver.
    pub(crate) fn new(
        use_test_dc: bool,
        credentials: Option<(i32, String)>,
        data_dir: PathBuf,
    ) -> Result<Self, AdapterError> {
        let (command_sender, command_receiver) = mpsc::channel(COMMAND_CHANNEL_CAPACITY);
        let (event_sender, event_receiver) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let (runtime_result_sender, runtime_result_receiver) = std::sync::mpsc::sync_channel(1);

        thread::Builder::new()
            .name("tdlib-supervisor".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_time()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = runtime_result_sender.send(Err(error));
                        return;
                    }
                };

                let client_id = tdlib_rs::create_client();
                let (update_sender, update_receiver) = mpsc::channel(UPDATE_CHANNEL_CAPACITY);
                let (overflow_sender, overflow_receiver) = mpsc::channel(1);
                let receiver_running = Arc::new(AtomicBool::new(true));
                let receiver_running_task = Arc::clone(&receiver_running);
                let receive_thread =
                    thread::Builder::new()
                        .name("tdlib-receive".into())
                        .spawn(move || {
                            receive_updates(
                                client_id,
                                update_sender,
                                overflow_sender,
                                receiver_running_task,
                            )
                        });

                let receive_thread = match receive_thread {
                    Ok(handle) => handle,
                    Err(error) => {
                        let _ = runtime_result_sender.send(Err(error));
                        return;
                    }
                };

                let _ = runtime_result_sender.send(Ok(()));
                runtime.block_on(run_supervisor(
                    client_id,
                    use_test_dc,
                    credentials,
                    data_dir,
                    command_receiver,
                    update_receiver,
                    overflow_receiver,
                    event_sender,
                ));
                receiver_running.store(false, Ordering::Release);
                if receive_thread.join().is_err() {
                    error!("TDLib update receiver thread stopped unexpectedly");
                }
            })?;

        runtime_result_receiver
            .recv()
            .map_err(|error| AdapterError::Runtime(std::io::Error::other(error)))??;

        Ok(Self {
            commands: command_sender,
            events: SubscriptionSource {
                id: NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed),
                receiver: Arc::new(Mutex::new(event_receiver)),
            },
        })
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        let (commands, _) = mpsc::channel(1);
        let (_, events) = mpsc::channel(1);
        Self {
            commands,
            events: SubscriptionSource {
                id: NEXT_SUBSCRIPTION_ID.fetch_add(1, Ordering::Relaxed),
                receiver: Arc::new(Mutex::new(events)),
            },
        }
    }

    /// Starts TDLib authorization and sets its internal log verbosity.
    pub(crate) async fn initialize(&self) -> Result<(), AdapterCommandError> {
        self.commands
            .send(BackendCommand::Initialize)
            .await
            .map_err(|_| AdapterCommandError::WorkerStopped)
    }

    /// Requests a proper Telegram logout; TDLib reports completion as `Closed`.
    pub(crate) async fn log_out(&self) -> Result<(), AdapterCommandError> {
        self.commands
            .send(BackendCommand::LogOut)
            .await
            .map_err(|_| AdapterCommandError::WorkerStopped)
    }

    /// Sends a phone number to TDLib without exposing backend types to the app.
    pub(crate) async fn submit_phone(&self, phone: String) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::SubmitPhone(phone)).await
    }

    /// Sends a login code to TDLib without exposing backend types to the app.
    pub(crate) async fn submit_code(&self, code: String) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::SubmitCode(code)).await
    }

    /// Sends a 2FA password to TDLib without exposing backend types to the app.
    pub(crate) async fn submit_password(
        &self,
        password: String,
    ) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::SubmitPassword(password))
            .await
    }

    /// Loads the requested range of main-list chat summaries.
    pub(crate) async fn load_chat_page(
        &self,
        query: ChatPageQuery,
    ) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::LoadChatPage(query)).await
    }

    /// Opens a chat in TDLib and requests its latest messages.
    pub(crate) async fn open_chat(&self, chat_id: i64) -> Result<(), AdapterCommandError> {
        self.commands
            .send(BackendCommand::OpenChat(chat_id))
            .await
            .map_err(|_| AdapterCommandError::WorkerStopped)
    }

    /// Reports the most recent incoming message viewed in the open chat.
    pub(crate) async fn mark_read(
        &self,
        address: MessageAddress,
        request_id: u64,
    ) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::MarkRead {
            address,
            request_id,
        })
        .await
    }

    /// Loads the messages before the oldest message currently shown in a chat.
    pub(crate) async fn load_older_messages(
        &self,
        chat_id: i64,
        before_message_id: i64,
        request_id: u64,
    ) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::LoadOlderMessages {
            chat_id,
            before_message_id,
            request_id,
        })
        .await
    }

    /// Sends text with an optional reply to a message in the same chat.
    pub(crate) async fn send_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: String,
        reply_to: Option<i64>,
    ) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::SendText {
            chat_id,
            message_id,
            text,
            reply_to,
        })
        .await
    }

    /// Resolves the original messages for a bounded batch of reply rows.
    pub(crate) async fn resolve_replies(
        &self,
        chat_id: i64,
        message_ids: Vec<i64>,
    ) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::ResolveReplies {
            chat_id,
            message_ids,
        })
        .await
    }

    /// Asks Telegram to forward one message with its attribution intact.
    pub(crate) async fn forward(
        &self,
        from_chat_id: i64,
        message_id: i64,
        to_chat_id: i64,
    ) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::Forward {
            from_chat_id,
            message_id,
            to_chat_id,
        })
        .await
    }

    /// Inspects or changes a message through the TDLib worker.
    pub(crate) async fn message_action(
        &self,
        address: MessageAddress,
        request_id: u64,
        action: MessageAction,
    ) -> Result<(), AdapterCommandError> {
        self.send_command(BackendCommand::MessageAction {
            address,
            request_id,
            action,
        })
        .await
    }

    async fn send_command(&self, command: BackendCommand) -> Result<(), AdapterCommandError> {
        self.commands
            .send(command)
            .await
            .map_err(|_| AdapterCommandError::WorkerStopped)
    }

    /// Streams shared authorization events into iced.
    pub(crate) fn subscription(&self) -> Subscription<BackendEvent> {
        Subscription::run_with(self.events.clone(), subscription_stream)
    }
}

fn subscription_stream(
    source: &SubscriptionSource,
) -> iced::futures::stream::BoxStream<'static, BackendEvent> {
    let receiver = Arc::clone(&source.receiver);

    Box::pin(stream::channel(
        EVENT_CHANNEL_CAPACITY,
        async move |mut output| {
            let mut receiver = receiver.lock().await;
            while let Some(event) = receiver.recv().await {
                if output.send(event).await.is_err() {
                    break;
                }
            }
        },
    ))
}

fn receive_updates(
    client_id: i32,
    sender: mpsc::Sender<(tdlib_rs::enums::Update, i32)>,
    overflow_sender: mpsc::Sender<()>,
    running: Arc<AtomicBool>,
) {
    while running.load(Ordering::Acquire) {
        if let Some(update) = tdlib_rs::receive() {
            match sender.try_send(update) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    // Never block TDLib's only receive thread: responses to
                    // pending API requests are delivered through the same call.
                    let _ = overflow_sender.try_send(());
                }
                Err(mpsc::error::TrySendError::Closed(_)) => break,
            }
        }
    }
    tracing::debug!(client_id, "TDLib update receiver stopped");
}

#[allow(clippy::too_many_arguments)]
async fn run_supervisor(
    initial_client_id: i32,
    use_test_dc: bool,
    credentials: Option<(i32, String)>,
    data_dir: PathBuf,
    mut commands: mpsc::Receiver<BackendCommand>,
    mut updates: mpsc::Receiver<(tdlib_rs::enums::Update, i32)>,
    mut update_overflows: mpsc::Receiver<()>,
    events: mpsc::Sender<BackendEvent>,
) {
    let mut client_id = initial_client_id;
    let mut client_closed = false;
    let mut account_user_id = None;
    let mut open_chat_id = None;
    let mut pending_message_ids = HashMap::new();
    let mut pending_forward = None;
    let mut background_requests = JoinSet::new();
    let mut history_page_job: Option<(u64, tokio::task::AbortHandle)> = None;
    let mut chat_page_job: Option<tokio::task::AbortHandle> = None;
    loop {
        tokio::select! {
            Some(result) = background_requests.join_next(), if !background_requests.is_empty() => {
                if let Ok(event) = result
                    && events.send(event).await.is_err()
                {
                    break;
                }
            }
            command = commands.recv() => match command {
                Some(BackendCommand::Initialize) => {
                    if client_closed {
                        client_id = tdlib_rs::create_client();
                        client_closed = false;
                    }
                    if tdlib_rs::functions::set_log_verbosity_level(2, client_id).await.is_err() {
                        warn!("could not lower TDLib log verbosity");
                    }
                    match tdlib_rs::functions::get_authorization_state(client_id).await {
                        Ok(state) => {
                            let state = mapping::authorization_state_from_tdlib(&state);
                            if events.send(BackendEvent::AuthorizationChanged(state)).await.is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            warn!(code = error.code, "could not read TDLib authorization state");
                        }
                    }
                }
                Some(BackendCommand::LogOut) => {
                    if let Some((_, job)) = history_page_job.take() {
                        job.abort();
                    }
                    if let Err(error) = tdlib_rs::functions::log_out(client_id).await {
                        let _ = events.send(BackendEvent::AuthorizationError(
                            mapping::authorization_error(error.message),
                        )).await;
                    }
                }
                Some(BackendCommand::SubmitPhone(phone)) => {
                    if let Err(error) = tdlib_rs::functions::set_authentication_phone_number(
                        phone,
                        None,
                        client_id,
                    ).await {
                        let _ = events.send(BackendEvent::AuthorizationError(
                            mapping::authorization_error(error.message),
                        )).await;
                    }
                }
                Some(BackendCommand::SubmitCode(code)) => {
                    if let Err(error) = tdlib_rs::functions::check_authentication_code(code, client_id).await {
                        let _ = events.send(BackendEvent::AuthorizationError(
                            mapping::authorization_error(error.message),
                        )).await;
                    }
                }
                Some(BackendCommand::SubmitPassword(password)) => {
                    if let Err(error) = tdlib_rs::functions::check_authentication_password(password, client_id).await {
                        let _ = events.send(BackendEvent::AuthorizationError(
                            mapping::authorization_error(error.message),
                        )).await;
                    }
                }
                Some(BackendCommand::LoadChatPage(query)) => {
                    if client_closed {
                        let _ = events.send(BackendEvent::ChatPageFailed {
                            request_id: query.request_id,
                            message: "Telegram is no longer connected.".into(),
                        }).await;
                        continue;
                    }
                    if let Some(job) = chat_page_job.take() {
                        job.abort();
                    }
                    if account_user_id.is_none() {
                        account_user_id = tokio::time::timeout(
                            INITIALIZATION_TIMEOUT, tdlib_rs::functions::get_me(client_id),
                        ).await.ok().and_then(Result::ok).map(mapping::user_id);
                    }
                    chat_page_job = Some(background_requests.spawn(
                        load_chat_page(query, client_id, account_user_id),
                    ));
                }
                Some(BackendCommand::OpenChat(chat_id)) => {
                    if let Some((_, job)) = history_page_job.take() {
                        job.abort();
                    }
                    open_chat_id = None;
                    if let Err(error) = tdlib_rs::functions::open_chat(chat_id, client_id).await {
                        let _ = events.send(BackendEvent::ChatHistoryError {
                            chat_id,
                            message: format!("Could not open chat (error {}).", error.code),
                        }).await;
                        continue;
                    }
                    open_chat_id = Some(chat_id);
                    let result = tokio::time::timeout(
                        HISTORY_TIMEOUT, load_initial_history(chat_id, client_id),
                    ).await;
                    let event = match result {
                        Ok(Ok((messages, has_older_messages))) => BackendEvent::ChatHistoryLoaded {
                            chat_id, messages, has_older_messages,
                        },
                        Ok(Err(error)) => BackendEvent::ChatHistoryError {
                            chat_id,
                            message: format!("Could not load chat history (error {}).", error.code),
                        },
                        Err(_) => BackendEvent::ChatHistoryError {
                            chat_id, message: "Loading chat history timed out.".into(),
                        },
                    };
                    let _ = events.send(event).await;
                }
                Some(BackendCommand::MarkRead { address, request_id }) => {
                    if open_chat_id == Some(address.chat_id) && !client_closed {
                        background_requests.spawn(async move {
                            let accepted = match tdlib_rs::functions::view_messages(
                                address.chat_id,
                                vec![address.message_id],
                                Some(tdlib_rs::enums::MessageSource::ChatHistory),
                                false,
                                client_id,
                            ).await {
                                Ok(()) => true,
                                Err(error) => {
                                    tracing::debug!(code = error.code, "TDLib rejected read receipt");
                                    false
                                }
                            };
                            BackendEvent::ReadReceiptFinished { request_id, accepted }
                        });
                    } else {
                        let _ = events.send(BackendEvent::ReadReceiptFinished {
                            request_id, accepted: false,
                        }).await;
                    }
                }
                Some(BackendCommand::LoadOlderMessages { chat_id, before_message_id, request_id }) => {
                    if open_chat_id != Some(chat_id) || client_closed {
                        let _ = events.send(BackendEvent::OlderChatHistoryError {
                            request_id,
                            message: "The chat is no longer open.".into(),
                        }).await;
                        continue;
                    }
                    if history_page_job
                        .as_ref()
                        .is_some_and(|(active, _)| *active >= request_id)
                    {
                        continue;
                    }
                    if let Some((_, job)) = history_page_job.take() {
                        job.abort();
                    }
                    let job = background_requests.spawn(
                        load_older_history(chat_id, before_message_id, request_id, client_id),
                    );
                    history_page_job = Some((request_id, job));
                }
                Some(BackendCommand::SendText { chat_id, message_id, text, reply_to }) => {
                    let content = mapping::input_text_content(text.clone(), true);
                    match tdlib_rs::functions::send_message(
                        chat_id, None, reply_to.map(mapping::input_reply), None, content, client_id,
                    ).await {
                        Ok(tdlib_rs::enums::Message::Message(message)) => {
                            pending_message_ids.insert((chat_id, message.id), (message_id, text));
                        }
                        Err(error) => {
                            warn!(code = error.code, "Telegram rejected a text message");
                            let _ = events.send(BackendEvent::MessageSendRejected {
                                chat_id,
                                message_id,
                                text,
                                reason: "Telegram couldn't send the message.".into(),
                            }).await;
                        }
                    }
                }
                Some(BackendCommand::ResolveReplies { chat_id, message_ids }) => {
                    background_requests.spawn(resolve_replies(chat_id, message_ids, client_id));
                }
                Some(BackendCommand::MessageAction { address, request_id, action }) => {
                    let inspect = matches!(action, MessageAction::Inspect);
                    let request = perform_message_action(address, request_id, action, client_id);
                    if inspect {
                        let _ = events.send(request.await).await;
                    } else {
                        background_requests.spawn(request);
                    }
                }
                Some(BackendCommand::Forward { from_chat_id, message_id, to_chat_id }) => {
                    match forward_message(from_chat_id, message_id, to_chat_id, client_id).await {
                        Ok(message) if matches!(message.sending_state, Some(tdlib_rs::enums::MessageSendingState::Pending(_))) => {
                            pending_forward = Some((to_chat_id, message.id));
                        }
                        Ok(message) if message.sending_state.is_none() => {
                            let message = map_message(message, client_id).await;
                            let _ = events.send(BackendEvent::NewMessage { chat_id: to_chat_id, message }).await;
                            let _ = events.send(BackendEvent::ForwardFinished { result: Ok(()), discarded_message: None }).await;
                        }
                        result => {
                            let reason = result.err().unwrap_or("Telegram couldn't forward the message.");
                            let _ = events.send(BackendEvent::ForwardFinished { result: Err(reason.into()), discarded_message: None }).await;
                        }
                    }
                }
                None => break,
            },
            update = updates.recv() => match update {
                Some((update, update_client_id)) if update_client_id == client_id => {
                    if let Some(state) = mapping::authorization_state(&update) {
                        if state == AuthorizationState::Closed {
                            if let Some((_, job)) = history_page_job.take() {
                                job.abort();
                            }
                            if let Some(job) = chat_page_job.take() {
                                job.abort();
                            }
                            client_closed = true;
                            account_user_id = None;
                            open_chat_id = None;
                            pending_message_ids.clear();
                            pending_forward = None;
                        }
                        if events.send(BackendEvent::AuthorizationChanged(state)).await.is_err() {
                            break;
                        }
                        if state == AuthorizationState::WaitTdlibParameters
                            && let Some(error) = configure_tdlib(
                                client_id,
                                use_test_dc,
                                credentials.as_ref(),
                                &data_dir,
                            ).await {
                            let _ = events
                                .send(BackendEvent::AuthorizationError(error.into()))
                                .await;
                        }
                    }
                    if let Some(chat_id) = mapping::chat_removed_from_main_list(&update) {
                        if events.send(BackendEvent::ChatRemoved(chat_id)).await.is_err() {
                            break;
                        }
                    } else if let tdlib_rs::enums::Update::NewChat(new_chat) = &update {
                        match mapping::chat(&new_chat.chat, account_user_id) {
                            Some(chat) => {
                                if events.send(BackendEvent::ChatUpdated(chat)).await.is_err() {
                                    break;
                                }
                                tracing::debug!("published chat summary from new-chat update");
                            }
                            None => tracing::debug!(
                                "new-chat update has no main-list position yet"
                            ),
                        }
                    } else if let Some(chat_id) = mapping::chat_update_id(&update) {
                        match tdlib_rs::functions::get_chat(chat_id, client_id).await {
                            Ok(tdlib_rs::enums::Chat::Chat(td_chat)) => {
                                match mapping::chat(&td_chat, account_user_id) {
                                    Some(chat) => {
                                        if events.send(BackendEvent::ChatUpdated(chat)).await.is_err() {
                                            break;
                                        }
                                        tracing::debug!("published chat summary from chat update");
                                    }
                                    None => tracing::debug!(
                                        "chat update has no main-list position; skipped summary"
                                    ),
                                }
                            }
                            Err(error) => warn!(
                                code = error.code,
                                "could not read chat after Telegram update"
                            ),
                        }
                    }
                    match update {
                        tdlib_rs::enums::Update::NewMessage(update) => {
                            let chat_id = update.message.chat_id;
                            let mut message = map_message(update.message, client_id).await;
                            if let Some((id, _)) = pending_message_ids.get(&(chat_id, message.id)) {
                                message.id = *id;
                            }
                            if events.send(BackendEvent::NewMessage { chat_id, message }).await.is_err() {
                                break;
                            }
                        }
                        tdlib_rs::enums::Update::MessageSendSucceeded(update) => {
                            let chat_id = update.message.chat_id;
                            let old_message_id = update.old_message_id;
                            let key = (chat_id, old_message_id);
                            let local_message_id = pending_message_ids.remove(&key).map(|(id, _)| id);
                            let message = map_message(update.message, client_id).await;
                            if events.send(BackendEvent::MessageSendSucceeded {
                                chat_id, old_message_id, local_message_id, message,
                            }).await.is_err() {
                                break;
                            }
                            if pending_forward == Some(key) {
                                pending_forward = None;
                                let _ = events.send(BackendEvent::ForwardFinished { result: Ok(()), discarded_message: None }).await;
                            }
                        }
                        tdlib_rs::enums::Update::MessageSendFailed(update) => {
                            let chat_id = update.message.chat_id;
                            let old_message_id = update.old_message_id;
                            let key = (chat_id, old_message_id);
                            if let Some((message_id, text)) = pending_message_ids.remove(&key)
                                && events.send(BackendEvent::MessageSendRejected {
                                    chat_id, message_id, text,
                                    reason: "Telegram couldn't send the message.".into(),
                                }).await.is_err()
                            {
                                break;
                            }
                            if pending_forward == Some(key) {
                                pending_forward = None;
                                let _ = events.send(BackendEvent::ForwardFinished {
                                    result: Err("Telegram couldn't forward the message.".into()),
                                    discarded_message: Some(key),
                                }).await;
                            }
                        }
                        update => {
                            if let Some(event) = mapping::message_change(update)
                                && events.send(event).await.is_err()
                            {
                                break;
                            }
                        }
                    }
                }
                Some(_) => {}
                None => break,
            },
            overflow = update_overflows.recv() => match overflow {
                Some(()) => {
                    warn!("TDLib update queue overflowed; resynchronizing authorization and chats");
                    if pending_forward.take().is_some() {
                        let _ = events.send(BackendEvent::ForwardFinished {
                            result: Err("Telegram did not confirm whether the message was forwarded.".into()),
                            discarded_message: None,
                        }).await;
                    }
                    match tdlib_rs::functions::get_authorization_state(client_id).await {
                        Ok(td_state) => {
                            let state = mapping::authorization_state_from_tdlib(&td_state);
                            if state == AuthorizationState::Closed {
                                if let Some((_, job)) = history_page_job.take() {
                                    job.abort();
                                }
                                if let Some(job) = chat_page_job.take() {
                                    job.abort();
                                }
                                client_closed = true;
                                account_user_id = None;
                                open_chat_id = None;
                                pending_message_ids.clear();
                                pending_forward = None;
                            }
                            if events.send(BackendEvent::AuthorizationChanged(state)).await.is_err() {
                                break;
                            }
                            if state == AuthorizationState::Ready {
                                let _ = events.send(BackendEvent::ChatListInvalidated).await;
                            } else if state == AuthorizationState::WaitTdlibParameters
                                && let Some(error) = configure_tdlib(
                                    client_id,
                                    use_test_dc,
                                    credentials.as_ref(),
                                    &data_dir,
                                ).await
                            {
                                let _ = events
                                    .send(BackendEvent::AuthorizationError(error.into()))
                                    .await;
                            }
                        }
                        Err(error) => {
                            warn!(code = error.code, "could not resynchronize TDLib authorization state");
                        }
                    }
                }
                None => break,
            },
        }
    }
}

async fn history_before(
    chat_id: i64,
    before: i64,
    limit: usize,
    client_id: i32,
) -> Result<Vec<tdlib_rs::types::Message>, tdlib_rs::types::Error> {
    // The API may return the boundary itself or temporarily have no rows.
    // Retry once if the response does not include a strictly older ID.
    for _ in 0..2 {
        let tdlib_rs::enums::Messages::Messages(page) = tdlib_rs::functions::get_chat_history(
            chat_id,
            before,
            0,
            (limit + usize::from(before != 0)) as i32,
            false,
            client_id,
        )
        .await?;
        let messages: Vec<_> = page
            .messages
            .into_iter()
            .flatten()
            .filter(|message| before == 0 || message.id < before)
            .take(limit)
            .collect();
        if !messages.is_empty() {
            return Ok(messages);
        }
    }
    Ok(Vec::new())
}

async fn load_initial_history(
    chat_id: i64,
    client_id: i32,
) -> Result<(Vec<ChatMessage>, bool), tdlib_rs::types::Error> {
    let mut messages = Vec::with_capacity(CHAT_HISTORY_LIMIT);
    let mut before = 0;
    while messages.len() < CHAT_HISTORY_LIMIT {
        let page = history_before(
            chat_id,
            before,
            CHAT_HISTORY_LIMIT - messages.len(),
            client_id,
        )
        .await?;
        let Some(oldest) = page.iter().map(|message| message.id).min() else {
            break;
        };
        before = oldest;
        messages.extend(page);
    }
    let has_older_messages = messages.len() == CHAT_HISTORY_LIMIT;
    let mut messages = map_history_messages(messages, client_id).await;
    messages.sort_by_key(|message| (message.date, message.id));
    Ok((messages, has_older_messages))
}

async fn load_older_history(
    chat_id: i64,
    before: i64,
    request_id: u64,
    client_id: i32,
) -> BackendEvent {
    match tokio::time::timeout(HISTORY_TIMEOUT, async {
        let page = history_before(chat_id, before, CHAT_HISTORY_LIMIT, client_id).await?;
        let mut messages = map_history_messages(page, client_id).await;
        messages.sort_by_key(|message| (message.date, message.id));
        Ok::<_, tdlib_rs::types::Error>(messages)
    })
    .await
    {
        Ok(Ok(messages)) => BackendEvent::OlderChatHistoryLoaded {
            request_id,
            has_older_messages: !messages.is_empty(),
            messages,
        },
        Ok(Err(error)) => BackendEvent::OlderChatHistoryError {
            request_id,
            message: format!("Could not load older messages (error {}).", error.code),
        },
        Err(_) => BackendEvent::OlderChatHistoryError {
            request_id,
            message: "Loading older messages timed out.".into(),
        },
    }
}

async fn resolve_sender_name(sender: mapping::MessageSenderReference, client_id: i32) -> String {
    match sender {
        mapping::MessageSenderReference::Outgoing => "You".into(),
        mapping::MessageSenderReference::User(user_id) => {
            match tdlib_rs::functions::get_user(user_id, client_id).await {
                Ok(tdlib_rs::enums::User::User(user)) => mapping::user_name(&user),
                Err(_) => "Unknown sender".into(),
            }
        }
        mapping::MessageSenderReference::Chat(sender_chat_id) => {
            match tdlib_rs::functions::get_chat(sender_chat_id, client_id).await {
                Ok(tdlib_rs::enums::Chat::Chat(chat)) => mapping::chat_title(&chat),
                Err(_) => "Unknown sender".into(),
            }
        }
    }
}

async fn perform_message_action(
    address: MessageAddress,
    request_id: u64,
    action: MessageAction,
    client_id: i32,
) -> BackendEvent {
    let MessageAddress {
        chat_id,
        message_id,
    } = address;
    let result = match action {
        MessageAction::Inspect => {
            let result =
                tdlib_rs::functions::get_message_properties(chat_id, message_id, client_id)
                    .await
                    .map(mapping::message_actions)
                    .map_err(|_| "Couldn't load message actions.".into());
            return BackendEvent::MessageActionsLoaded { request_id, result };
        }
        MessageAction::Edit(text) => {
            let content = mapping::input_text_content(text, false);
            tdlib_rs::functions::edit_message_text(chat_id, message_id, content, client_id)
                .await
                .map(|_| ())
                .or_else(|error| match error.message.as_str() {
                    "MESSAGE_NOT_MODIFIED" => Ok(()),
                    _ => Err("Telegram couldn't edit the message.".into()),
                })
        }
        MessageAction::Delete(scope) => tdlib_rs::functions::delete_messages(
            chat_id,
            vec![message_id],
            scope == DeleteScope::ForEveryone,
            client_id,
        )
        .await
        .map_err(|_| "Telegram couldn't delete the message.".into()),
    };
    BackendEvent::MessageMutationFinished { request_id, result }
}

async fn resolve_replies(chat_id: i64, message_ids: Vec<i64>, client_id: i32) -> BackendEvent {
    let previews =
        iced::futures::future::join_all(message_ids.into_iter().map(|message_id| async move {
            let preview = tokio::time::timeout(REPLY_LOOKUP_TIMEOUT, async {
                let tdlib_rs::enums::Message::Message(message) =
                    tdlib_rs::functions::get_replied_message(chat_id, message_id, client_id)
                        .await
                        .ok()?;
                let sender =
                    resolve_sender_name(mapping::message_sender(&message), client_id).await;
                Some(mapping::original_preview(&message, &sender))
            })
            .await
            .ok()
            .flatten();
            (message_id, preview)
        }))
        .await;
    BackendEvent::ReplyPreviewsLoaded { chat_id, previews }
}

async fn forward_message(
    from_chat_id: i64,
    message_id: i64,
    to_chat_id: i64,
    client_id: i32,
) -> Result<tdlib_rs::types::Message, &'static str> {
    const ERROR: &str = "Telegram couldn't forward the message to this chat.";
    let tdlib_rs::enums::MessageProperties::MessageProperties(properties) =
        tdlib_rs::functions::get_message_properties(from_chat_id, message_id, client_id)
            .await
            .map_err(|_| ERROR)?;
    if !properties.can_be_forwarded {
        return Err("This message can't be forwarded.");
    }
    let tdlib_rs::enums::Messages::Messages(messages) = tdlib_rs::functions::forward_messages(
        to_chat_id,
        None,
        from_chat_id,
        vec![message_id],
        None,
        false,
        false,
        client_id,
    )
    .await
    .map_err(|_| ERROR)?;
    messages.messages.into_iter().flatten().next().ok_or(ERROR)
}

async fn map_message(message: tdlib_rs::types::Message, client_id: i32) -> ChatMessage {
    let sender = resolve_sender_name(mapping::message_sender(&message), client_id).await;
    let origin_name = match message
        .forward_info
        .as_ref()
        .and_then(|forward| mapping::origin_sender(&forward.origin))
    {
        Some(origin) => Some(resolve_sender_name(origin, client_id).await),
        None => None,
    };
    mapping::chat_message(message, sender, origin_name)
}

async fn map_history_messages(
    messages: Vec<tdlib_rs::types::Message>,
    client_id: i32,
) -> Vec<ChatMessage> {
    let mut sender_names: HashMap<mapping::MessageSenderReference, String> = HashMap::new();
    let mut mapped_messages = Vec::with_capacity(messages.len());
    for message in messages {
        let sender_name = cached_sender_name(
            mapping::message_sender(&message),
            client_id,
            &mut sender_names,
        )
        .await;
        let origin_name = match message
            .forward_info
            .as_ref()
            .and_then(|forward| mapping::origin_sender(&forward.origin))
        {
            Some(origin) => Some(cached_sender_name(origin, client_id, &mut sender_names).await),
            None => None,
        };
        mapped_messages.push(mapping::chat_message(message, sender_name, origin_name));
    }
    mapped_messages
}

async fn cached_sender_name(
    sender: mapping::MessageSenderReference,
    client_id: i32,
    names: &mut HashMap<mapping::MessageSenderReference, String>,
) -> String {
    if let Some(name) = names.get(&sender) {
        return name.clone();
    }
    let name = resolve_sender_name(sender, client_id).await;
    names.insert(sender, name.clone());
    name
}

async fn load_chat_page(
    query: ChatPageQuery,
    client_id: i32,
    account_user_id: Option<i64>,
) -> BackendEvent {
    match tokio::time::timeout(
        CHAT_PAGE_TIMEOUT,
        read_chat_page(query, client_id, account_user_id),
    )
    .await
    {
        Ok(Ok(event)) => event,
        Ok(Err(error)) => BackendEvent::ChatPageFailed {
            request_id: query.request_id,
            message: format!("Could not load chats (error {}).", error.code),
        },
        Err(_) => BackendEvent::ChatPageFailed {
            request_id: query.request_id,
            message: "Loading chats timed out.".into(),
        },
    }
}

async fn read_chat_page(
    query: ChatPageQuery,
    client_id: i32,
    account_user_id: Option<i64>,
) -> Result<BackendEvent, tdlib_rs::types::Error> {
    let list = Some(tdlib_rs::enums::ChatList::Main);
    // TDLib's offset-free snapshot provides IDs only. Hydrate just the
    // requested range. Keep a lookahead to avoid requesting an already cached
    // page from the server after scrolling back to it.
    let end = query.offset.saturating_add(query.limit).saturating_add(1);
    let limit = i32::try_from(end).unwrap_or(i32::MAX);
    let cached_ids = |limit| tdlib_rs::functions::get_chats(list.clone(), limit, client_id);
    let tdlib_rs::enums::Chats::Chats(mut snapshot) = cached_ids(limit).await?;
    let mut reached_end = false;
    let mut offset = chat_page_offset(query, &snapshot.chat_ids);
    let requested =
        i32::try_from(offset.saturating_add(query.limit).saturating_add(1)).unwrap_or(i32::MAX);
    if query.load_missing && requested as usize > snapshot.chat_ids.len() {
        let missing = (requested as usize - snapshot.chat_ids.len()).min(query.limit);
        match tdlib_rs::functions::load_chats(list.clone(), missing as i32, client_id).await {
            Ok(()) => {}
            Err(error) if error.code == 404 => reached_end = true,
            Err(error) => return Err(error),
        }
        let tdlib_rs::enums::Chats::Chats(current) = cached_ids(requested).await?;
        snapshot = current;
        offset = chat_page_offset(query, &snapshot.chat_ids);
    }
    // A snapshot may shrink while a page is requested. Clamp to what exists.
    offset = offset.min(snapshot.chat_ids.len());
    let next_offset = offset
        .saturating_add(query.limit)
        .min(snapshot.chat_ids.len());
    if query.load_missing && !reached_end && next_offset == offset {
        return Ok(BackendEvent::ChatPageFailed {
            request_id: query.request_id,
            message: "Telegram did not return the requested chat page.".into(),
        });
    }
    let has_more = next_offset < snapshot.chat_ids.len() || (query.load_missing && !reached_end);
    let mut chats = Vec::with_capacity(next_offset - offset);
    for &chat_id in &snapshot.chat_ids[offset..next_offset] {
        let tdlib_rs::enums::Chat::Chat(chat) =
            tdlib_rs::functions::get_chat(chat_id, client_id).await?;
        if let Some(chat) = mapping::chat(&chat, account_user_id) {
            chats.push(chat);
        }
    }
    Ok(BackendEvent::ChatPageLoaded {
        request_id: query.request_id,
        next_offset,
        next_after: next_offset
            .checked_sub(1)
            .map(|last| snapshot.chat_ids[last]),
        has_more,
        chats,
    })
}

fn chat_page_offset(query: ChatPageQuery, ids: &[i64]) -> usize {
    query
        .after
        .and_then(|after| ids.iter().position(|&id| id == after))
        .map_or(query.offset, |position| query.offset.min(position + 1))
}

async fn configure_tdlib(
    client_id: i32,
    use_test_dc: bool,
    credentials: Option<&(i32, String)>,
    data_dir: &std::path::Path,
) -> Option<&'static str> {
    let Some((api_id, api_hash)) = credentials else {
        warn!("TG_API_ID and TG_API_HASH are required to initialize TDLib");
        return Some("Set TG_API_ID and TG_API_HASH in .env, then restart the app.");
    };

    info!("sending TDLib initialization parameters");
    let database_directory = data_dir.to_string_lossy().into_owned();
    match tokio::time::timeout(
        INITIALIZATION_TIMEOUT,
        tdlib_rs::functions::set_tdlib_parameters(
            use_test_dc,
            database_directory,
            String::new(),
            String::new(),
            false,
            false,
            false,
            false,
            *api_id,
            api_hash.clone(),
            "en".into(),
            "tg-desktop-rs".into(),
            String::new(),
            env!("CARGO_PKG_VERSION").into(),
            client_id,
        ),
    )
    .await
    {
        Ok(Ok(())) => {
            info!("TDLib accepted its initialization parameters");
            None
        }
        Ok(Err(error)) => {
            error!(
                code = error.code,
                "TDLib rejected its initialization parameters"
            );
            Some(
                "Telegram rejected its initialization parameters. Check the API credentials and test server setting.",
            )
        }
        Err(_) => {
            error!("TDLib did not respond to initialization parameters within 10 seconds");
            Some(
                "Telegram initialization timed out after 10 seconds. Check your connection, then restart the app.",
            )
        }
    }
}
