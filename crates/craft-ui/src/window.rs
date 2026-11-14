//! Native windowing and event loop integration for CraftBrowsing using winit + egui.

use std::sync::Arc;

use egui_glow::winit::EguiGlow;
use glutin::prelude::GlSurface;
use tracing::info;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::raw_window_handle::HasWindowHandle;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::BrowserApp;
use craft_core::container::ContainerId;
use craft_core::media::MediaAction;

/// Owns the glutin GL context and window for one browser window.
struct GlWindow {
    window: Arc<Window>,
    gl_context: glutin::context::PossiblyCurrentContext,
    gl_surface: glutin::surface::Surface<glutin::surface::WindowSurface>,
    gl: Arc<glow::Context>,
}

impl GlWindow {
    unsafe fn new(event_loop: &ActiveEventLoop) -> Self {
        use glutin::context::NotCurrentGlContext as _;
        use glutin::display::GetGlDisplay as _;
        use glutin::display::GlDisplay as _;

        let window_attributes = WindowAttributes::default()
            .with_title("CraftBrowsing - Pure-Rust Servo Web Browser")
            .with_inner_size(LogicalSize::new(1280.0, 720.0))
            .with_visible(false);

        let config_template = glutin::config::ConfigTemplateBuilder::new()
            .prefer_hardware_accelerated(None)
            .with_depth_size(0)
            .with_stencil_size(0)
            .with_transparency(false);

        let (window, gl_config) = glutin_winit::DisplayBuilder::new()
            .with_preference(glutin_winit::ApiPreference::FallbackEgl)
            .with_window_attributes(Some(window_attributes.clone()))
            .build(event_loop, config_template, |mut configs| {
                configs.next().expect("no matching GL config")
            })
            .expect("failed to create GL config");

        let gl_display = gl_config.display();

        let raw_handle = window
            .as_ref()
            .and_then(|w| w.window_handle().ok())
            .map(|h| h.as_raw());

        let context_attributes = glutin::context::ContextAttributesBuilder::new().build(raw_handle);
        let fallback_attributes = glutin::context::ContextAttributesBuilder::new()
            .with_context_api(glutin::context::ContextApi::Gles(None))
            .build(raw_handle);

        let not_current = unsafe {
            gl_display
                .create_context(&gl_config, &context_attributes)
                .unwrap_or_else(|_| {
                    gl_display
                        .create_context(&gl_config, &fallback_attributes)
                        .expect("failed to create GL context")
                })
        };

        let window = window.unwrap_or_else(|| {
            glutin_winit::finalize_window(event_loop, window_attributes, &gl_config)
                .expect("failed to finalize window")
        });
        let window = Arc::new(window);

        let (width, height): (u32, u32) = window.inner_size().into();
        let width = std::num::NonZeroU32::new(width.max(1)).unwrap();
        let height = std::num::NonZeroU32::new(height.max(1)).unwrap();

        let surface_attributes =
            glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                .build(
                    window.window_handle().expect("window handle").as_raw(),
                    width,
                    height,
                );

        let gl_surface = unsafe {
            gl_display
                .create_window_surface(&gl_config, &surface_attributes)
                .expect("failed to create window surface")
        };

        let gl_context = not_current
            .make_current(&gl_surface)
            .expect("failed to make GL context current");

        let _ = gl_surface.set_swap_interval(
            &gl_context,
            glutin::surface::SwapInterval::Wait(std::num::NonZeroU32::MIN),
        );

        let gl = unsafe {
            glow::Context::from_loader_function(|s| {
                let s = std::ffi::CString::new(s).expect("cstring");
                gl_display.get_proc_address(&s)
            })
        };

        Self {
            window,
            gl_context,
            gl_surface,
            gl: Arc::new(gl),
        }
    }

    fn resize(&self, new_size: winit::dpi::PhysicalSize<u32>) {
        use glutin::surface::GlSurface as _;
        let width = std::num::NonZeroU32::new(new_size.width.max(1)).unwrap();
        let height = std::num::NonZeroU32::new(new_size.height.max(1)).unwrap();
        self.gl_surface.resize(&self.gl_context, width, height);
    }

    fn swap_buffers(&self) {
        use glutin::surface::GlSurface as _;
        let _ = self.gl_surface.swap_buffers(&self.gl_context);
    }
}

/// Winit application runner embedding the CraftBrowsing browser shell and egui chrome.
pub struct BrowserWindowApp {
    app: BrowserApp,
    gl_window: Option<GlWindow>,
    egui: Option<EguiGlow>,
    modifiers: ModifiersState,
}

impl BrowserWindowApp {
    pub fn new(app: BrowserApp) -> Self {
        Self {
            app,
            gl_window: None,
            egui: None,
            modifiers: ModifiersState::empty(),
        }
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut app_state = BrowserWindowApp::new(BrowserApp::new());
        event_loop.run_app(&mut app_state)?;

        Ok(())
    }
}

pub fn run_window_event_loop() -> Result<(), Box<dyn std::error::Error>> {
    BrowserWindowApp::run()
}

impl ApplicationHandler for BrowserWindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gl_window.is_none() {
            let gl_window = unsafe { GlWindow::new(event_loop) };

            let egui = EguiGlow::new(event_loop, Arc::clone(&gl_window.gl), None, None, true);
            egui.egui_ctx.set_visuals(egui::Visuals::dark());

            gl_window.window.set_visible(true);
            info!(
                "Created native OS window: {}x{}",
                gl_window.window.inner_size().width,
                gl_window.window.inner_size().height
            );

            self.gl_window = Some(gl_window);
            self.egui = Some(egui);

            if self.app.tabs.is_empty() {
                self.app
                    .open_tab("craft://welcome", ContainerId::default_container());
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gl_window) = self.gl_window.as_mut() else {
            return;
        };
        let Some(egui) = self.egui.as_mut() else {
            return;
        };

        let response = egui.on_window_event(&gl_window.window, &event);

        match event {
            WindowEvent::CloseRequested => {
                info!("Window close requested. Exiting CraftBrowsing.");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                gl_window.resize(new_size);
                gl_window.window.request_redraw();
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if !response.consumed {
                    self.handle_keyboard_shortcut(&event);
                }
            }
            WindowEvent::RedrawRequested => {
                let _ = self.app.poll_engine_events();

                if let Some(title) = self.app.media_window_title() {
                    gl_window.window.set_title(&title);
                }

                let media_count = self.app.media_hub.len();
                let app = &mut self.app;
                egui.run(&gl_window.window, |ui| {
                    egui::Panel::top("toolbar").show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("CraftBrowsing");
                            ui.separator();
                            ui.label(format!("Media: {}", media_count));
                            if ui.button("Play/Pause").clicked() {
                                app.dispatch_media_action(None, MediaAction::TogglePlayPause);
                            }
                        });
                    });
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.label("Browser viewport placeholder");
                        ui.label("Engine integration pending.");
                    });
                });

                unsafe {
                    use glow::HasContext as _;
                    gl_window.gl.clear_color(0.1, 0.1, 0.12, 1.0);
                    gl_window.gl.clear(glow::COLOR_BUFFER_BIT);
                }

                egui.paint(&gl_window.window);
                gl_window.swap_buffers();

                if egui.egui_ctx.has_requested_repaint() {
                    gl_window.window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(gl_window) = self.gl_window.as_ref() {
            gl_window.window.request_redraw();
        }
    }
}

impl BrowserWindowApp {
    fn handle_keyboard_shortcut(&mut self, event: &KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        match &event.logical_key {
            Key::Character(c) if c.to_ascii_lowercase() == "t" => {
                info!("Shortcut: New Tab");
                let tab_id = self
                    .app
                    .open_tab("craft://welcome", ContainerId::default_container());
                info!("Created tab: {:?}", tab_id);
            }
            Key::Character(c) if c.to_ascii_lowercase() == "w" => {
                if let Some(active_id) = self.app.active_tab_id.clone() {
                    info!("Shortcut: Close Tab {:?}", active_id);
                    self.app.close_tab(&active_id);
                }
            }
            Key::Character(c) if c.to_ascii_lowercase() == "r" => {
                if let Some(active_id) = self.app.active_tab_id.as_ref() {
                    info!("Shortcut: Reload Tab {:?}", active_id);
                    let _ = self
                        .app
                        .cmd_sender
                        .send(craft_engine::EngineCommand::Reload {
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
            Key::Named(NamedKey::Space)
                if (self.modifiers.control_key() || self.modifiers.super_key())
                    && self.modifiers.shift_key() =>
            {
                info!("Shortcut: Media Toggle Play/Pause");
                self.app
                    .dispatch_media_action(None, MediaAction::TogglePlayPause);
            }
            Key::Named(NamedKey::ArrowRight)
                if (self.modifiers.control_key() || self.modifiers.super_key())
                    && self.modifiers.shift_key() =>
            {
                info!("Shortcut: Media Skip Forward");
                self.app
                    .dispatch_media_action(None, MediaAction::SkipForward(10_000));
            }
            Key::Named(NamedKey::ArrowLeft)
                if (self.modifiers.control_key() || self.modifiers.super_key())
                    && self.modifiers.shift_key() =>
            {
                info!("Shortcut: Media Skip Backward");
                self.app
                    .dispatch_media_action(None, MediaAction::SkipBackward(10_000));
            }
            Key::Character(c)
                if (self.modifiers.control_key() || self.modifiers.super_key())
                    && self.modifiers.shift_key()
                    && c.to_ascii_lowercase() == "m" =>
            {
                info!("Shortcut: Media Toggle Mute");
                self.app
                    .dispatch_media_action(None, MediaAction::ToggleMute);
            }
            Key::Character(c)
                if (self.modifiers.control_key() || self.modifiers.super_key())
                    && self.modifiers.shift_key()
                    && c.to_ascii_lowercase() == "p" =>
            {
                info!("Shortcut: Media Enter PiP");
                self.app.dispatch_media_action(None, MediaAction::EnterPip);
            }
            _ => {}
        }
    }
}
