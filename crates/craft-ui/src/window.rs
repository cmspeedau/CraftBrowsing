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

                let app = &mut self.app;

                // Sync tab manager from app tabs.
                app.tab_manager
                    .sync_from_tabs(&app.tabs, app.active_tab_id.as_ref());

                let media_count = app.media_hub.len();
                let tabs: Vec<(craft_core::tab::TabId, String, bool)> = app
                    .tab_manager
                    .get_render_tree()
                    .iter()
                    .map(|v| (v.tab_id.clone(), v.title.clone(), v.is_active))
                    .collect();
                let active_url = app
                    .active_tab_id
                    .as_ref()
                    .and_then(|id| app.tabs.iter().find(|t| &t.id == id))
                    .map(|t| t.url.clone())
                    .unwrap_or_default();

                egui.run(&gl_window.window, |ui| {
                    // --- Top toolbar: omnibox + media ---
                    egui::Panel::top("toolbar").show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("CraftBrowsing");
                            ui.separator();

                            // Container selector
                            let current_container = app
                                .active_tab_id
                                .as_ref()
                                .and_then(|id| app.tabs.iter().find(|t| &t.id == id))
                                .map(|t| t.container_id.clone())
                                .unwrap_or_else(
                                    craft_core::container::ContainerId::default_container,
                                );
                            ui.label(format!("[{}]", current_container.0));
                            ui.separator();

                            // Omnibox
                            let mut url_text = if app.omnibox.is_focused {
                                app.omnibox.input_text.clone()
                            } else {
                                active_url.clone()
                            };
                            let response = ui.add(
                                egui::TextEdit::singleline(&mut url_text)
                                    .desired_width(ui.available_width() - 120.0)
                                    .hint_text("Search or enter URL"),
                            );
                            if response.gained_focus() {
                                app.omnibox.is_focused = true;
                                app.omnibox.input_text = active_url.clone();
                            }
                            if response.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            {
                                let input = if app.omnibox.is_focused {
                                    app.omnibox.input_text.clone()
                                } else {
                                    url_text.clone()
                                };
                                app.submit_omnibox(&input);
                                app.omnibox.is_focused = false;
                            }
                            // Update omnibox text while typing
                            if response.changed() {
                                app.omnibox.input_text = url_text.clone();
                            }

                            ui.separator();
                            // Media indicator
                            ui.label(format!("Media: {}", media_count));
                            if ui.button("\u{25B6}/\u{23F8}").clicked() {
                                app.dispatch_media_action(None, MediaAction::TogglePlayPause);
                            }
                        });
                    });

                    // --- Tab strip (left side panel) ---
                    egui::Panel::left("tabs")
                        .default_size(200.0)
                        .show(ui, |ui| {
                            ui.heading("Tabs");
                            ui.separator();
                            for (tab_id, title, is_active) in &tabs {
                                let label = if *is_active {
                                    format!("> {}", title)
                                } else {
                                    title.clone()
                                };
                                if ui.selectable_label(*is_active, &label).clicked() {
                                    app.select_tab(tab_id);
                                }
                            }
                            ui.separator();
                            if ui.button("+ New Tab").clicked() {
                                app.open_tab(
                                    "craft://welcome",
                                    craft_core::container::ContainerId::default_container(),
                                );
                            }
                        });

                    // --- Media hub (right side panel) ---
                    egui::Panel::right("media_hub")
                        .default_size(260.0)
                        .show(ui, |ui| {
                            ui.heading("Media Sessions");
                            ui.separator();
                            if app.media_hub.is_empty() {
                                ui.label("No active media.");
                            } else {
                                for session in app.media_hub.sessions.clone() {
                                    ui.group(|ui| {
                                        ui.label(session.display_title());
                                        ui.label(format!(
                                            "{} \u{2022} {}",
                                            session.origin, session.container_id.0
                                        ));
                                        ui.label(format!(
                                            "{:?} \u{2022} {}",
                                            session.state,
                                            if session.is_live() {
                                                "LIVE".to_string()
                                            } else {
                                                format!(
                                                    "{} / {}",
                                                    format_ms(session.position_ms),
                                                    session
                                                        .duration_ms
                                                        .map(format_ms)
                                                        .unwrap_or_else(|| "?".to_string())
                                                )
                                            }
                                        ));
                                        ui.horizontal(|ui| {
                                            if ui.button("\u{25C0}\u{25C0}").clicked() {
                                                app.dispatch_media_action(
                                                    Some(session.id.clone()),
                                                    MediaAction::SkipBackward(10_000),
                                                );
                                            }
                                            if ui
                                                .button(if session.is_playing() {
                                                    "\u{23F8}"
                                                } else {
                                                    "\u{25B6}"
                                                })
                                                .clicked()
                                            {
                                                app.dispatch_media_action(
                                                    Some(session.id.clone()),
                                                    MediaAction::TogglePlayPause,
                                                );
                                            }
                                            if ui.button("\u{25B6}\u{25B6}").clicked() {
                                                app.dispatch_media_action(
                                                    Some(session.id.clone()),
                                                    MediaAction::SkipForward(10_000),
                                                );
                                            }
                                            if ui
                                                .button(if session.muted {
                                                    "\u{1F507}"
                                                } else {
                                                    "\u{1F50A}"
                                                })
                                                .clicked()
                                            {
                                                app.dispatch_media_action(
                                                    Some(session.id.clone()),
                                                    MediaAction::ToggleMute,
                                                );
                                            }
                                        });
                                        let mut vol = session.volume * 100.0;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut vol, 0.0..=100.0)
                                                    .text("Vol"),
                                            )
                                            .changed()
                                        {
                                            app.dispatch_media_action(
                                                Some(session.id.clone()),
                                                MediaAction::SetVolume(vol / 100.0),
                                            );
                                        }
                                    });
                                }
                            }
                        });

                    // --- Central viewport ---
                    egui::CentralPanel::default().show(ui, |ui| {
                        ui.label("Browser viewport");
                        ui.label("Servo rendering pending engine integration.");
                        ui.separator();
                        if let Some(active_id) = &app.active_tab_id {
                            if let Some(tab) = app.tabs.iter().find(|t| &t.id == active_id) {
                                ui.label(format!("Active tab: {}", tab.title));
                                ui.label(format!("URL: {}", tab.url));
                                ui.label(format!("Container: {}", tab.container_id.0));
                                ui.label(format!("State: {:?}", tab.state));
                            }
                        } else {
                            ui.label("No active tab. Press Ctrl+T to open one.");
                        }
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

fn format_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{}:{:02}", mins, secs)
}
