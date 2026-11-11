//! TDLib client lifecycle and bounded event delivery.

use std::{
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

use crate::domain::{AuthorizationState, BackendEvent};

use super::mapping;

static NEXT_SUBSCRIPTION_ID: AtomicU64 = AtomicU64::new(1);
const EVENT_CHANNEL_CAPACITY: usize = 64;
const UPDATE_CHANNEL_CAPACITY: usize = 64;
const COMMAND_CHANNEL_CAPACITY: usize = 8;

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

#[derive(Debug)]
enum BackendCommand {
    Initialize,
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
                let receiver_running = Arc::new(AtomicBool::new(true));
                let receiver_running_task = Arc::clone(&receiver_running);
                let receive_thread =
                    thread::Builder::new()
                        .name("tdlib-receive".into())
                        .spawn(move || {
                            receive_updates(client_id, update_sender, receiver_running_task)
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
    running: Arc<AtomicBool>,
) {
    while running.load(Ordering::Acquire) {
        if let Some(update) = tdlib_rs::receive()
            && sender.blocking_send(update).is_err()
        {
            break;
        }
    }
    tracing::debug!(client_id, "TDLib update receiver stopped");
}

async fn run_supervisor(
    client_id: i32,
    use_test_dc: bool,
    credentials: Option<(i32, String)>,
    data_dir: PathBuf,
    mut commands: mpsc::Receiver<BackendCommand>,
    mut updates: mpsc::Receiver<(tdlib_rs::enums::Update, i32)>,
    events: mpsc::Sender<BackendEvent>,
) {
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(BackendCommand::Initialize) => {
                    if tdlib_rs::functions::set_log_verbosity_level(2, client_id).await.is_err() {
                        warn!("could not lower TDLib log verbosity");
                    }
                }
                None => break,
            },
            update = updates.recv() => match update {
                Some((update, update_client_id)) if update_client_id == client_id => {
                    if let Some(state) = mapping::authorization_state(&update) {
                        if events.send(BackendEvent::AuthorizationChanged(state)).await.is_err() {
                            break;
                        }
                        if state == AuthorizationState::WaitTdlibParameters {
                            configure_tdlib(
                                client_id,
                                use_test_dc,
                                credentials.as_ref(),
                                &data_dir,
                            ).await;
                        }
                    }
                }
                Some(_) => {}
                None => break,
            }
        }
    }
}

async fn configure_tdlib(
    client_id: i32,
    use_test_dc: bool,
    credentials: Option<&(i32, String)>,
    data_dir: &std::path::Path,
) {
    let Some((api_id, api_hash)) = credentials else {
        warn!("TG_API_ID and TG_API_HASH are required to initialize TDLib");
        return;
    };

    info!("sending TDLib initialization parameters");
    let database_directory = data_dir.to_string_lossy().into_owned();
    match tokio::time::timeout(
        Duration::from_secs(60),
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
        Ok(Ok(())) => info!("TDLib accepted its initialization parameters"),
        Ok(Err(error)) => error!(
            code = error.code,
            "TDLib rejected its initialization parameters"
        ),
        Err(_) => error!("TDLib did not respond to initialization parameters within 60 seconds"),
    }
}
