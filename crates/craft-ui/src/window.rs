//! Native windowing and event loop integration for CraftBrowsing using winit.

use std::sync::Arc;
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowAttributes, WindowId};

use craft_core::container::ContainerId;
use craft_engine::CompositorSurface;
use crate::BrowserApp;

/// Winit application runner embedding the CraftBrowsing browser shell and native rendering surface.
pub struct BrowserWindowApp {
    app: BrowserApp,
    window: Option<Arc<Window>>,
    surface: Option<CompositorSurface>,
}

impl BrowserWindowApp {
    pub fn new(app: BrowserApp) -> Self {
        Self {
            app,
            window: None,
            surface: None,
        }
    }

    /// Run the native winit event loop.
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut app_state = BrowserWindowApp::new(BrowserApp::new());
        event_loop.run_app(&mut app_state)?;

        Ok(())
    }

    fn handle_keyboard_shortcut(&mut self, event: &KeyEvent) {
        if event.state == ElementState::Pressed {
            match &event.logical_key {
                // Ctrl/Cmd + T: Open new tab
                Key::Character(c) if c.to_ascii_lowercase() == "t" => {
                    info!("Shortcut: New Tab");
                    let tab_id = self.app.open_tab("craft://welcome", ContainerId::default_container());
                    info!("Created tab: {:?}", tab_id);
                }
                // Ctrl/Cmd + W: Close active tab
                Key::Character(c) if c.to_ascii_lowercase() == "w" => {
                    if let Some(active_id) = self.app.active_tab_id.clone() {
                        info!("Shortcut: Close Tab {:?}", active_id);
                        self.app.close_tab(&active_id);
                    }
                }
                // Ctrl/Cmd + R: Reload current tab
                Key::Character(c) if c.to_ascii_lowercase() == "r" => {
                    if let Some(active_id) = self.app.active_tab_id.as_ref() {
                        info!("Shortcut: Reload Tab {:?}", active_id);
                        let _ = self.app.cmd_sender.send(craft_engine::EngineCommand::Reload {
                            tab_id: active_id.clone(),
                            bypass_cache: false,
                        });
                    }
                }
                Key::Named(NamedKey::Escape) => {
                    if let Some(active_id) = self.app.active_tab_id.as_ref() {
                        let _ = self.app.cmd_sender.send(craft_engine::EngineCommand::Stop {
                            tab_id: active_id.clone(),
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

impl ApplicationHandler for BrowserWindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = WindowAttributes::default()
                .with_title("CraftBrowsing - Pure-Rust Servo Web Browser")
                .with_inner_size(LogicalSize::new(1280.0, 720.0));

            match event_loop.create_window(window_attributes) {
                Ok(window) => {
                    let window_ref = Arc::new(window);
                    let size = window_ref.inner_size();
                    let scale_factor = window_ref.scale_factor();

                    info!(
                        "Created native OS window: {}x{} @ {}x DPR",
                        size.width, size.height, scale_factor
                    );

                    let surface = CompositorSurface::new(size.width, size.height, scale_factor);
                    self.surface = Some(surface);

                    // Ensure an initial tab is open
                    if self.app.tabs.is_empty() {
                        self.app.open_tab("craft://welcome", ContainerId::default_container());
                    }

                    self.window = Some(window_ref);
                }
                Err(e) => {
                    error!("Failed to create native browser window: {:?}", e);
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Window close requested. Exiting CraftBrowsing.");
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(surface) = self.surface.as_mut() {
                    surface.resize(physical_size.width, physical_size.height);
                    info!(
                        "Native surface resized to {}x{}",
                        physical_size.width, physical_size.height
                    );

                    // Propagate resize to active tab in engine
                    if let Some(active_id) = self.app.active_tab_id.as_ref() {
                        let _ = self.app.cmd_sender.send(craft_engine::EngineCommand::ResizeViewport {
                            tab_id: active_id.clone(),
                            width: physical_size.width,
                            height: physical_size.height,
                        });
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(surface) = self.surface.as_mut() {
                    surface.update_scale_factor(scale_factor);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.handle_keyboard_shortcut(&event);
            }
            WindowEvent::RedrawRequested => {
                // Poll any pending events from the Servo Engine
                let events = self.app.poll_engine_events();
                for evt in events {
                    info!("Engine event handled in UI loop: {:?}", evt);
                }

                // Render active composited surface
                if let Some(surface) = self.surface.as_ref() {
                    surface.present();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self) {
        // Continuous engine event drain and frame trigger
        let _ = self.app.poll_engine_events();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}
