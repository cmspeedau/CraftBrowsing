//! Native windowing and event loop integration for CraftBrowsing using winit + Servo + egui.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use egui_glow::EguiGlow;
use euclid::Scale;

use servo::{
    EventLoopWaker, LoadStatus, OffscreenRenderingContext, RenderingContext, Servo, ServoBuilder,
    WebView, WebViewBuilder, WebViewDelegate, WindowRenderingContext,
};
use tracing::info;
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::BrowserApp;
use crate::omnibox::{OmniboxState, OmniboxTarget};
use craft_core::container::ContainerId;
use craft_core::media::MediaAction;

/// Custom user event used to wake the Servo event loop.
#[derive(Debug)]
struct WakerEvent;

/// Live page state shared between the Servo delegate and the chrome.
#[derive(Debug, Default)]
struct PageState {
    url: String,
    title: String,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
}

/// Delegate that forwards Servo frame-ready notifications to winit.
struct BrowserWebViewDelegate {
    window: Arc<Window>,
    new_frame: Arc<AtomicBool>,
    page: Arc<Mutex<PageState>>,
}

impl WebViewDelegate for BrowserWebViewDelegate {
    fn notify_new_frame_ready(&self, _: WebView) {
        self.new_frame.store(true, Ordering::Release);
        self.window.request_redraw();
    }

    fn notify_url_changed(&self, _: WebView, url: Url) {
        self.page.lock().unwrap().url = url.to_string();
        self.window.request_redraw();
    }

    fn notify_page_title_changed(&self, _: WebView, title: Option<String>) {
        self.page.lock().unwrap().title = title.unwrap_or_default();
        self.window.request_redraw();
    }

    fn notify_load_status_changed(&self, webview: WebView, status: LoadStatus) {
        {
            let mut page = self.page.lock().unwrap();
            page.loading = matches!(status, LoadStatus::Started | LoadStatus::HeadParsed);
            page.can_go_back = webview.can_go_back();
            page.can_go_forward = webview.can_go_forward();
        }
        self.window.request_redraw();
    }
}

/// Waker that posts a user event to the winit event loop.
#[derive(Clone)]
struct WakerProxy(winit::event_loop::EventLoopProxy<WakerEvent>);

impl EventLoopWaker for WakerProxy {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }

    fn wake(&self) {
        let _ = self.0.send_event(WakerEvent);
    }
}

/// Winit application runner embedding the CraftBrowsing browser shell.
pub struct BrowserWindowApp {
    app: BrowserApp,
    window: Option<Arc<Window>>,
    rendering_context: Option<Rc<WindowRenderingContext>>,
    servo: Option<Servo>,
    webview: Option<WebView>,
    offscreen: Option<Rc<OffscreenRenderingContext>>,
    egui: Option<EguiGlow>,
    new_frame: Arc<AtomicBool>,
    page: Arc<Mutex<PageState>>,
    omnibox_buffer: String,
    modifiers: ModifiersState,
    initial_url: String,
    event_proxy: Option<winit::event_loop::EventLoopProxy<WakerEvent>>,
}

impl BrowserWindowApp {
    pub fn new(app: BrowserApp) -> Self {
        Self {
            app,
            window: None,
            rendering_context: None,
            servo: None,
            webview: None,
            offscreen: None,
            egui: None,
            new_frame: Arc::new(AtomicBool::new(true)),
            page: Arc::new(Mutex::new(PageState::default())),
            omnibox_buffer: String::new(),
            modifiers: ModifiersState::empty(),
            initial_url: "https://example.com".to_string(),
            event_proxy: None,
        }
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        // Install rustls crypto provider before anything touches TLS.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        // Content-process gate: if launched as a content process, run it and exit.
        let mut token = None;
        let mut take_next = false;
        for arg in std::env::args() {
            if take_next {
                token = Some(arg);
                break;
            }
            if arg == "--content-process" {
                take_next = true;
            }
        }
        if let Some(tok) = token {
            servo::run_content_process(tok);
            return Ok(());
        }

        let event_loop = EventLoop::with_user_event().build()?;
        event_loop.set_control_flow(ControlFlow::Wait);

        let mut app_state = BrowserWindowApp::new(BrowserApp::new());
        app_state.event_proxy = Some(event_loop.create_proxy());
        event_loop.run_app(&mut app_state)?;

        Ok(())
    }
}

pub fn run_window_event_loop() -> Result<(), Box<dyn std::error::Error>> {
    BrowserWindowApp::run()
}

impl ApplicationHandler<WakerEvent> for BrowserWindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = WindowAttributes::default()
            .with_title("CraftBrowsing - Pure-Rust Servo Web Browser")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));

        let window = Arc::new(
            event_loop
                .create_window(window_attributes)
                .expect("failed to create window"),
        );

        let display_handle = event_loop.display_handle().expect("display handle");
        let window_handle = window.window_handle().expect("window handle");

        let rendering_context = Rc::new(
            WindowRenderingContext::new(display_handle, window_handle, window.inner_size())
                .expect("failed to create rendering context"),
        );
        rendering_context.make_current().expect("make_current");

        // Build Servo.
        let proxy = self.event_proxy.clone().expect("event proxy");
        let waker = WakerProxy(proxy);
        let opts = servo::Opts {
            multiprocess: false,
            sandbox: false,
            hard_fail: false,
            temporary_storage: true,
            ..Default::default()
        };
        let servo = ServoBuilder::default()
            .opts(opts)
            .event_loop_waker(Box::new(waker))
            .build();

        // Create offscreen context + webview.
        let offscreen = Rc::new(rendering_context.offscreen_context(window.inner_size()));
        let offscreen_rc: Rc<dyn RenderingContext> = offscreen.clone();

        let delegate = Rc::new(BrowserWebViewDelegate {
            window: window.clone(),
            new_frame: self.new_frame.clone(),
            page: self.page.clone(),
        });

        let url =
            Url::parse(&self.initial_url).unwrap_or_else(|_| Url::parse("about:blank").unwrap());

        self.page.lock().unwrap().url = url.to_string();
        self.omnibox_buffer = url.to_string();

        let webview = WebViewBuilder::new(&servo, offscreen_rc)
            .url(url)
            .hidpi_scale_factor(Scale::new(window.scale_factor() as f32))
            .delegate(delegate)
            .build();

        // egui context using Servo's GL.
        let gl = rendering_context.glow_gl_api();
        let egui = EguiGlow::new(event_loop, gl, None, None, true);
        egui.egui_ctx.set_visuals(egui::Visuals::dark());

        window.set_visible(true);
        info!(
            "Created window {}x{} with Servo",
            window.inner_size().width,
            window.inner_size().height
        );

        self.window = Some(window);
        self.rendering_context = Some(rendering_context);
        self.servo = Some(servo);
        self.webview = Some(webview);
        self.offscreen = Some(offscreen);
        self.egui = Some(egui);

        if self.app.tabs.is_empty() {
            self.app
                .open_tab("craft://welcome", ContainerId::default_container());
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(window) = self.window.clone() else {
            return;
        };
        let Some(rendering_context) = self.rendering_context.clone() else {
            return;
        };
        let Some(egui) = self.egui.as_mut() else {
            return;
        };

        let response = egui.on_window_event(&window, &event);

        match event {
            WindowEvent::CloseRequested => {
                info!("Window close requested. Exiting CraftBrowsing.");
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                rendering_context.resize(new_size);
                if let Some(offscreen) = self.offscreen.as_ref() {
                    offscreen.resize(new_size);
                }
                if let Some(webview) = self.webview.as_ref() {
                    webview.resize(new_size);
                }
                window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(webview) = self.webview.as_ref() {
                    webview.set_hidpi_scale_factor(Scale::new(scale_factor as f32));
                }
                window.request_redraw();
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
                // Pump Servo.
                if let Some(servo) = self.servo.as_ref() {
                    servo.spin_event_loop();
                }

                let _ = self.app.poll_engine_events();

                // Make GL context current.
                rendering_context.make_current().expect("make_current");

                // Render the webview into its offscreen context.
                if let Some(webview) = self.webview.as_ref() {
                    webview.paint();
                }

                // Clear the window framebuffer.
                unsafe {
                    use glow::HasContext as _;
                    let gl = rendering_context.glow_gl_api();
                    gl.clear_color(0.1, 0.1, 0.12, 1.0);
                    gl.clear(glow::COLOR_BUFFER_BIT);
                }

                // Blit the offscreen webview into the window before egui paints.
                if let Some(offscreen) = self.offscreen.as_ref()
                    && let Some(blit) = offscreen.render_to_parent_callback()
                {
                    let size = rendering_context.size();
                    let gl = rendering_context.glow_gl_api();
                    let rect = euclid::default::Rect::new(
                        euclid::default::Point2D::new(0, 0),
                        euclid::default::Size2D::new(size.width as i32, size.height as i32),
                    );
                    blit(&gl, rect);
                }

                // Run egui UI over the blitted page.
                let app = &mut self.app;
                let webview_nav = self.webview.clone();
                let page = self.page.clone();
                let buffer = &mut self.omnibox_buffer;
                if let Some(egui) = self.egui.as_mut() {
                    let window_ref = window.clone();
                    egui.run(&window_ref, |ui| {
                        // --- Top toolbar: nav buttons + omnibox + media count ---
                        egui::Panel::top("toolbar").show_inside(ui, |ui| {
                            let (loading, can_back, can_fwd) = {
                                let p = page.lock().unwrap();
                                (p.loading, p.can_go_back, p.can_go_forward)
                            };
                            ui.horizontal(|ui| {
                                if let Some(wv) = &webview_nav {
                                    if ui.add_enabled(can_back, egui::Button::new("←")).clicked()
                                    {
                                        let _ = wv.go_back(1);
                                    }
                                    if ui.add_enabled(can_fwd, egui::Button::new("→")).clicked() {
                                        let _ = wv.go_forward(1);
                                    }
                                    if ui.button("⟳").clicked() {
                                        wv.reload();
                                    }
                                }
                                if loading {
                                    ui.label("(loading…)");
                                }

                                ui.separator();
                                let resp = ui.add(
                                    egui::TextEdit::singleline(buffer)
                                        .desired_width(ui.available_width() - 60.0)
                                        .hint_text("Search or enter URL"),
                                );
                                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    && resp.has_focus();
                                if enter {
                                    let target = match OmniboxState::resolve_input(buffer) {
                                        OmniboxTarget::Url(u)
                                        | OmniboxTarget::Internal(u)
                                        | OmniboxTarget::Search {
                                            resolved_url: u, ..
                                        } => u,
                                    };
                                    *buffer = target.clone();
                                    if let Some(wv) = &webview_nav
                                        && let Ok(parsed) = Url::parse(&target)
                                    {
                                        wv.load(parsed);
                                    }
                                    resp.surrender_focus();
                                }

                                ui.separator();
                                let media_count = app.media_hub.len();
                                ui.label(format!("Media: {}", media_count));
                                if ui.button("⏯").clicked() {
                                    app.dispatch_media_action(None, MediaAction::TogglePlayPause);
                                }
                            });
                        });

                        // --- Left tab sidebar ---
                        egui::Panel::left("tabs").show_inside(ui, |ui| {
                            ui.heading("Tabs");
                            ui.separator();
                            app.tab_manager
                                .sync_from_tabs(&app.tabs, app.active_tab_id.as_ref());
                            let tree = app.tab_manager.get_render_tree();
                            for item in tree {
                                let label = if item.is_active {
                                    format!("▸ {}", item.title)
                                } else {
                                    format!("  {}", item.title)
                                };
                                if ui.selectable_label(item.is_active, label).clicked() {
                                    app.select_tab(&item.tab_id);
                                }
                            }
                            ui.separator();
                            if ui.button("+ New Tab").clicked() {
                                app.open_tab("craft://welcome", ContainerId::default_container());
                            }
                        });

                        // --- Right media hub ---
                        egui::Panel::right("media_hub")
                            .default_size(260.0)
                            .show_inside(ui, |ui| {
                                ui.heading("Media Sessions");
                                ui.separator();
                                if app.media_hub.is_empty() {
                                    ui.label("No active media.");
                                } else {
                                    for session in app.media_hub.sessions.clone() {
                                        ui.group(|ui| {
                                            ui.label(session.display_title());
                                            ui.label(format!(
                                                "{} • {}",
                                                session.origin, session.container_id.0
                                            ));
                                            ui.label(format!(
                                                "{:?} • {}",
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
                                                if ui.button("⏪").clicked() {
                                                    app.dispatch_media_action(
                                                        Some(session.id.clone()),
                                                        MediaAction::SkipBackward(10_000),
                                                    );
                                                }
                                                let glyph = if session.is_playing() {
                                                    "⏸"
                                                } else {
                                                    "▶"
                                                };
                                                if ui.button(glyph).clicked() {
                                                    app.dispatch_media_action(
                                                        Some(session.id.clone()),
                                                        MediaAction::TogglePlayPause,
                                                    );
                                                }
                                                if ui.button("⏩").clicked() {
                                                    app.dispatch_media_action(
                                                        Some(session.id.clone()),
                                                        MediaAction::SkipForward(10_000),
                                                    );
                                                }
                                                let mute =
                                                    if session.muted { "🔇" } else { "🔊" };
                                                if ui.button(mute).clicked() {
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

                        // --- Central viewport: transparent so the page shows ---
                        egui::CentralPanel::default()
                            .frame(egui::Frame::new().fill(egui::Color32::TRANSPARENT))
                            .show_inside(ui, |_ui| {});
                    });

                    egui.paint(&window_ref);
                }

                // Reflect page title in the window title.
                {
                    let p = self.page.lock().unwrap();
                    let title = if p.title.is_empty() {
                        format!("CraftBrowsing - {}", p.url)
                    } else {
                        format!("CraftBrowsing - {}", p.title)
                    };
                    window.set_title(&title);
                }

                rendering_context.present();

                if self.new_frame.swap(false, Ordering::AcqRel) {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: WakerEvent) {
        if let Some(servo) = self.servo.as_ref() {
            servo.spin_event_loop();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(servo) = self.servo.as_ref() {
            servo.spin_event_loop();
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
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
                    if let Some(webview) = self.webview.as_ref() {
                        webview.reload();
                    }
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

fn format_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}", mins, secs)
}
