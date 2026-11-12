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
use tokio::sync::{Mutex, mpsc};
use tracing::{error, info, warn};

use crate::domain::{AuthorizationState, BackendEvent, ChatMessage};

use super::mapping;

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);
const EVENT_CHANNEL_CAPACITY: usize = 64;
const UPDATE_CHANNEL_CAPACITY: usize = 64;
const COMMAND_CHANNEL_CAPACITY: usize = 8;
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);
const CHAT_LIST_LIMIT: i32 = 50;
const CHAT_HISTORY_LIMIT: usize = 50;

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
        self.send_authorization_command(BackendCommand::SubmitPhone(phone))
            .await
    }

    /// Sends a login code to TDLib without exposing backend types to the app.
    pub(crate) async fn submit_code(&self, code: String) -> Result<(), AdapterCommandError> {
        self.send_authorization_command(BackendCommand::SubmitCode(code))
            .await
    }

    /// Sends a 2FA password to TDLib without exposing backend types to the app.
    pub(crate) async fn submit_password(
        &self,
        password: String,
    ) -> Result<(), AdapterCommandError> {
        self.send_authorization_command(BackendCommand::SubmitPassword(password))
            .await
    }

    /// Opens a chat in TDLib and requests its latest messages.
    pub(crate) async fn open_chat(&self, chat_id: i64) -> Result<(), AdapterCommandError> {
        self.commands
            .send(BackendCommand::OpenChat(chat_id))
            .await
            .map_err(|_| AdapterCommandError::WorkerStopped)
    }

    async fn send_authorization_command(
        &self,
        command: BackendCommand,
    ) -> Result<(), AdapterCommandError> {
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
    let mut main_chats_loaded = false;
    loop {
        tokio::select! {
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
                            if state == AuthorizationState::Ready && !main_chats_loaded {
                                main_chats_loaded = load_main_chats(client_id, &events).await;
                            }
                        }
                        Err(error) => {
                            warn!(code = error.code, "could not read TDLib authorization state");
                        }
                    }
                }
                Some(BackendCommand::LogOut) => {
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
                Some(BackendCommand::OpenChat(chat_id)) => {
                    if let Err(error) = tdlib_rs::functions::open_chat(chat_id, client_id).await {
                        let _ = events.send(BackendEvent::ChatHistoryError {
                            chat_id,
                            message: format!("Could not open chat (error {}).", error.code),
                        }).await;
                        continue;
                    }
                    let mut messages: Vec<ChatMessage> = Vec::with_capacity(CHAT_HISTORY_LIMIT);
                    let mut sender_names: HashMap<mapping::MessageSenderReference, String> =
                        HashMap::new();
                    let mut from_message_id = 0;
                    let mut first_request = true;
                    loop {
                        match tdlib_rs::functions::get_chat_history(
                            chat_id, from_message_id, 0, CHAT_HISTORY_LIMIT as i32, false, client_id,
                        ).await {
                            Ok(tdlib_rs::enums::Messages::Messages(page)) => {
                                let page_messages: Vec<_> = page.messages.into_iter().flatten().collect();
                                if page_messages.is_empty() {
                                    if first_request {
                                        first_request = false;
                                        continue;
                                    }
                                    break;
                                }
                                first_request = false;
                                let oldest_id = page_messages.iter().map(|message| message.id).min().unwrap_or_default();
                                let received = page_messages.len();
                                if from_message_id != 0 && oldest_id >= from_message_id {
                                    break;
                                }
                                for message in page_messages {
                                    if !messages.iter().any(|loaded| loaded.id == message.id) {
                                        let sender = mapping::message_sender(&message);
                                        let sender_name = if let Some(name) = sender_names.get(&sender) {
                                            name.clone()
                                        } else {
                                            let name = match sender {
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
                                            };
                                            sender_names.insert(sender, name.clone());
                                            name
                                        };
                                        messages.push(mapping::chat_message(message, sender_name));
                                    }
                                }
                                if messages.len() >= CHAT_HISTORY_LIMIT || received >= CHAT_HISTORY_LIMIT || oldest_id == 0 {
                                    break;
                                }
                                from_message_id = oldest_id;
                            }
                            Err(error) => {
                                let _ = events.send(BackendEvent::ChatHistoryError {
                                    chat_id,
                                    message: format!("Could not load chat history (error {}).", error.code),
                                }).await;
                                messages.clear();
                                break;
                            }
                        }
                    }
                    messages.sort_by_key(|message| (message.date, message.id));
                    messages.truncate(CHAT_HISTORY_LIMIT);
                    let _ = events.send(BackendEvent::ChatHistoryLoaded { chat_id, messages }).await;
                }
                None => break,
            },
            update = updates.recv() => match update {
                Some((update, update_client_id)) if update_client_id == client_id => {
                    if let Some(state) = mapping::authorization_state(&update) {
                        if state == AuthorizationState::Closed {
                            client_closed = true;
                            main_chats_loaded = false;
                        }
                        if events.send(BackendEvent::AuthorizationChanged(state)).await.is_err() {
                            break;
                        }
                        if state == AuthorizationState::Ready && !main_chats_loaded {
                            main_chats_loaded = load_main_chats(client_id, &events).await;
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
                        match mapping::chat(&new_chat.chat) {
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
                                match mapping::chat(&td_chat) {
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
                    if let Some(sender) = mapping::new_message_sender(&update) {
                        let sender_name = match sender {
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
                        };
                        if let Some((chat_id, message)) = mapping::new_message(&update, sender_name)
                            && events.send(BackendEvent::NewMessage { chat_id, message }).await.is_err() {
                            break;
                        }
                    }
                }
                Some(_) => {}
                None => break,
            },
            overflow = update_overflows.recv() => match overflow {
                Some(()) => {
                    warn!("TDLib update queue overflowed; resynchronizing authorization and chats");
                    match tdlib_rs::functions::get_authorization_state(client_id).await {
                        Ok(td_state) => {
                            let state = mapping::authorization_state_from_tdlib(&td_state);
                            if state == AuthorizationState::Closed {
                                client_closed = true;
                                main_chats_loaded = false;
                            }
                            if events.send(BackendEvent::AuthorizationChanged(state)).await.is_err() {
                                break;
                            }
                            if state == AuthorizationState::Ready {
                                main_chats_loaded = load_main_chats(client_id, &events).await;
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

async fn load_main_chats(client_id: i32, events: &mpsc::Sender<BackendEvent>) -> bool {
    let load_result = tdlib_rs::functions::load_chats(
        Some(tdlib_rs::enums::ChatList::Main),
        CHAT_LIST_LIMIT,
        client_id,
    )
    .await;
    let load_was_expected = match load_result {
        Ok(()) => true,
        Err(error) if error.code == 404 => true,
        Err(error) => {
            warn!(code = error.code, "could not load Telegram chat list");
            false
        }
    };

    let chat_ids = match tdlib_rs::functions::get_chats(
        Some(tdlib_rs::enums::ChatList::Main),
        CHAT_LIST_LIMIT,
        client_id,
    )
    .await
    {
        Ok(tdlib_rs::enums::Chats::Chats(chats)) => chats.chat_ids,
        Err(error) => {
            warn!(
                code = error.code,
                "could not read cached Telegram chat list"
            );
            return load_was_expected;
        }
    };
    tracing::info!(count = chat_ids.len(), "loaded Telegram chat list snapshot");

    let mut published_count = 0;
    for chat_id in chat_ids {
        match tdlib_rs::functions::get_chat(chat_id, client_id).await {
            Ok(tdlib_rs::enums::Chat::Chat(chat)) => {
                if let Some(chat) = mapping::chat(&chat) {
                    if events.send(BackendEvent::ChatUpdated(chat)).await.is_err() {
                        return false;
                    }
                    published_count += 1;
                }
            }
            Err(error) => warn!(code = error.code, "could not read cached Telegram chat"),
        }
    }
    tracing::info!(
        count = published_count,
        "published Telegram chats to application"
    );
    load_was_expected
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
