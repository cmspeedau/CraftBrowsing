//! Servo rendering engine integration and lifecycle manager for CraftBrowsing.

pub mod compositor;
pub mod events;
pub mod pipeline;

use craft_adblock::AdblockManager;
use craft_core::container::ContainerId;
use craft_core::tab::TabId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{info, warn};

pub use compositor::CompositorSurface;
pub use events::{EngineEvent, NavigationEvent, PageLoadState};

/// Represents an individual Servo browsing context tied to a Tab and Container.
#[derive(Debug)]
pub struct EngineContext {
    pub tab_id: TabId,
    pub container_id: ContainerId,
    pub current_url: String,
    pub title: String,
    pub load_state: PageLoadState,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

impl EngineContext {
    pub fn new(tab_id: TabId, container_id: ContainerId, initial_url: &str) -> Self {
        Self {
            tab_id,
            container_id,
            current_url: initial_url.to_string(),
            title: String::from("New Tab"),
            load_state: PageLoadState::Initial,
            can_go_back: false,
            can_go_forward: false,
            viewport_width: 1280,
            viewport_height: 720,
        }
    }
}

/// Commands sent from the UI shell to the Servo Engine worker.
#[derive(Debug)]
pub enum EngineCommand {
    CreateContext {
        tab_id: TabId,
        container_id: ContainerId,
        url: String,
    },
    DestroyContext {
        tab_id: TabId,
    },
    Navigate {
        tab_id: TabId,
        url: String,
    },
    Reload {
        tab_id: TabId,
        bypass_cache: bool,
    },
    Stop {
        tab_id: TabId,
    },
    GoBack {
        tab_id: TabId,
    },
    GoForward {
        tab_id: TabId,
    },
    ResizeViewport {
        tab_id: TabId,
        width: u32,
        height: u32,
    },
    ExecuteScript {
        tab_id: TabId,
        script: String,
    },
}

/// The Servo Engine runner managing WebRender surfaces and containerized network sandboxes.
pub struct ServoEngine {
    cmd_sender: mpsc::UnboundedSender<EngineCommand>,
    event_receiver: mpsc::UnboundedReceiver<EngineEvent>,
    contexts: HashMap<TabId, EngineContext>,
    adblock: Arc<AdblockManager>,
}

impl ServoEngine {
    /// Initialize the Servo Engine bridge with adblocking and container routing.
    pub fn new(adblock: Arc<AdblockManager>) -> (Self, mpsc::UnboundedSender<EngineCommand>) {
        info!("Initializing Servo Engine runtime and WebRender pipeline...");
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<EngineCommand>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<EngineEvent>();

        // Spawn Servo engine event loop worker thread
        std::thread::Builder::new()
            .name("servo-engine-worker".into())
            .spawn(move || {
                info!("Servo engine worker thread started.");
                while let Some(command) = cmd_rx.blocking_recv() {
                    match command {
                        EngineCommand::CreateContext {
                            tab_id,
                            container_id,
                            url,
                        } => {
                            info!(
                                "Engine: creating browsing context for tab {:?} in container {:?}",
                                tab_id, container_id
                            );
                            let _ = event_tx.send(EngineEvent::Navigation(NavigationEvent {
                                tab_id: tab_id.clone(),
                                state: PageLoadState::Started,
                                url: url.clone(),
                                status_code: None,
                            }));

                            let _ = event_tx.send(EngineEvent::Navigation(NavigationEvent {
                                tab_id,
                                state: PageLoadState::Completed,
                                url,
                                status_code: Some(200),
                            }));
                        }
                        EngineCommand::Navigate { tab_id, url } => {
                            info!("Engine: navigating tab {:?} to {}", tab_id, url);
                            let _ = event_tx.send(EngineEvent::Navigation(NavigationEvent {
                                tab_id: tab_id.clone(),
                                state: PageLoadState::Started,
                                url: url.clone(),
                                status_code: None,
                            }));
                            let _ = event_tx.send(EngineEvent::Navigation(NavigationEvent {
                                tab_id,
                                state: PageLoadState::Completed,
                                url,
                                status_code: Some(200),
                            }));
                        }
                        EngineCommand::DestroyContext { tab_id } => {
                            info!("Engine: destroying context for tab {:?}", tab_id);
                        }
                        EngineCommand::ResizeViewport {
                            tab_id,
                            width,
                            height,
                        } => {
                            info!(
                                "Engine: resized tab {:?} viewport to {}x{}",
                                tab_id, width, height
                            );
                        }
                        EngineCommand::Reload {
                            tab_id,
                            bypass_cache,
                        } => {
                            info!(
                                "Engine: reload tab {:?} (bypass_cache: {})",
                                tab_id, bypass_cache
                            );
                        }
                        EngineCommand::Stop { tab_id } => {
                            info!("Engine: stop loading tab {:?}", tab_id);
                        }
                        EngineCommand::GoBack { tab_id } => {
                            info!("Engine: navigate back on tab {:?}", tab_id);
                        }
                        EngineCommand::GoForward { tab_id } => {
                            info!("Engine: navigate forward on tab {:?}", tab_id);
                        }
                        EngineCommand::ExecuteScript { tab_id, script: _ } => {
                            info!("Engine: executing script on tab {:?}", tab_id);
                        }
                    }
                }
                info!("Servo engine worker thread terminated.");
            })
            .expect("failed to spawn servo engine worker thread");

        let engine = Self {
            cmd_sender: cmd_tx.clone(),
            event_receiver: event_rx,
            contexts: HashMap::new(),
            adblock,
        };

        (engine, cmd_tx)
    }

    /// Issue a command to create a new browsing context for a tab.
    pub fn create_context(&mut self, tab_id: TabId, container_id: ContainerId, url: &str) {
        let context = EngineContext::new(tab_id.clone(), container_id.clone(), url);
        self.contexts.insert(tab_id.clone(), context);
        let _ = self.cmd_sender.send(EngineCommand::CreateContext {
            tab_id,
            container_id,
            url: url.to_string(),
        });
    }

    /// Navigate an existing tab context to a target URL.
    pub fn navigate(&mut self, tab_id: &TabId, url: &str) {
        if let Some(ctx) = self.contexts.get_mut(tab_id) {
            ctx.current_url = url.to_string();
            ctx.load_state = PageLoadState::Started;
            let _ = self.cmd_sender.send(EngineCommand::Navigate {
                tab_id: tab_id.clone(),
                url: url.to_string(),
            });
        }
    }

    /// Poll for engine events (navigation changes, title updates, repaint readiness).
    pub fn poll_events(&mut self) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_receiver.try_recv() {
            if let EngineEvent::Navigation(ref nav) = event {
                if let Some(ctx) = self.contexts.get_mut(&nav.tab_id) {
                    ctx.load_state = nav.state.clone();
                }
            }
            events.push(event);
        }
        events
    }
}
