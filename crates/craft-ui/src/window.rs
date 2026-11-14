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
    home_url: String,
    media_hub_open: bool,
    menu_open: bool,
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
            home_url: "https://example.com".to_string(),
            media_hub_open: false,
            menu_open: false,
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

impl Drop for BrowserWindowApp {
    fn drop(&mut self) {
        // Teardown order matters: the egui glow painter and Servo/WebRender both
        // free GL resources, so the surfman context must stay alive and current
        // until they are done, and the window surface must outlive the context.
        // Dropping these fields in declaration order would free the context first
        // and segfault on exit.
        if let (Some(egui), Some(rc)) = (self.egui.as_mut(), self.rendering_context.as_ref()) {
            let _ = rc.make_current();
            egui.destroy();
        }
        self.webview = None;
        self.offscreen = None;
        self.egui = None;
        self.servo = None;
        self.rendering_context = None;
        self.window = None;
    }
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

        // The webview renders straight into the window's RenderingContext, like
        // servo's winit_minimal example. egui composites its chrome over those
        // pixels in the same GL context, so there is no offscreen FBO to blit
        // (the old blit path silently produced chrome over a blank window).
        let webview_context: Rc<dyn RenderingContext> = rendering_context.clone();

        let delegate = Rc::new(BrowserWebViewDelegate {
            window: window.clone(),
            new_frame: self.new_frame.clone(),
            page: self.page.clone(),
        });

        let url =
            Url::parse(&self.initial_url).unwrap_or_else(|_| Url::parse("about:blank").unwrap());

        self.page.lock().unwrap().url = url.to_string();
        self.omnibox_buffer = url.to_string();

        let webview = WebViewBuilder::new(&servo, webview_context)
            .url(url)
            .hidpi_scale_factor(Scale::new(window.scale_factor() as f32))
            .delegate(delegate)
            .build();

        // egui context using Servo's GL, styled to the Firefox chrome palette.
        let gl = rendering_context.glow_gl_api();
        let egui = EguiGlow::new(event_loop, gl, None, None, true);
        egui.egui_ctx.set_visuals(firefox_visuals());

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
        self.offscreen = None;
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

                // Render the page into the window's default framebuffer. This is
                // the same GL context egui paints its chrome into, so egui's
                // meshes composite over the page (transparent central panel).
                if let Some(webview) = self.webview.as_ref() {
                    webview.paint();
                }

                // Run egui UI over the painted page.
                let app = &mut self.app;
                let webview_nav = self.webview.clone();
                let page = self.page.clone();
                let buffer = &mut self.omnibox_buffer;
                let webview_home = self.home_url.clone();
                let media_open = &mut self.media_hub_open;
                let menu_open = &mut self.menu_open;
                if let Some(egui) = self.egui.as_mut() {
                    let window_ref = window.clone();
                    egui.run(&window_ref, |ui| {
                        // ---- Row 1: tab strip (rounded tabs, container accents) ----
                        egui::Panel::top("tabstrip")
                            .frame(
                                egui::Frame::new()
                                    .fill(hex("#f0f0f4"))
                                    .inner_margin(egui::Margin {
                                        top: 6,
                                        left: 8,
                                        right: 8,
                                        bottom: 0,
                                    })
                                    .stroke(egui::Stroke::NONE),
                            )
                            .show_inside(ui, |ui| {
                                app.tab_manager
                                    .sync_from_tabs(&app.tabs, app.active_tab_id.as_ref());
                                let tree = app.tab_manager.get_render_tree();
                                let mut select_id: Option<craft_core::tab::TabId> = None;
                                let mut close_id: Option<craft_core::tab::TabId> = None;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    for item in &tree {
                                        let (_name, color_hex, icon) =
                                            app.container_identity(&item.container_id);
                                        let is_default = item.container_id.0
                                            == craft_core::container::ContainerId::DEFAULT;
                                        let label = if item.title.is_empty() {
                                            item.url.clone()
                                        } else {
                                            item.title.clone()
                                        };
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 4.0;
                                            if !is_default {
                                                // Real container accent, from Container.color_hex.
                                                ui.label(
                                                    egui::RichText::new("▌")
                                                        .color(hex(&color_hex))
                                                        .size(16.0),
                                                );
                                                ui.label(egui::RichText::new(icon).size(12.0));
                                            }
                                            let tab = ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new(&label).size(13.0),
                                                    )
                                                    .fill(if item.is_active {
                                                        egui::Color32::WHITE
                                                    } else {
                                                        egui::Color32::TRANSPARENT
                                                    })
                                                    .stroke(egui::Stroke::NONE)
                                                    .corner_radius(egui::CornerRadius {
                                                        nw: 8,
                                                        ne: 8,
                                                        sw: 0,
                                                        se: 0,
                                                    })
                                                    .min_size(egui::vec2(150.0, 30.0)),
                                                )
                                                .on_hover_text(&label);
                                            if tab.clicked() {
                                                select_id = Some(item.tab_id.clone());
                                            }
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        egui::RichText::new("×").size(14.0),
                                                    )
                                                    .fill(egui::Color32::TRANSPARENT)
                                                    .stroke(egui::Stroke::NONE),
                                                )
                                                .on_hover_text("Close tab")
                                                .clicked()
                                            {
                                                close_id = Some(item.tab_id.clone());
                                            }
                                        });
                                    }
                                    if ui
                                        .add(
                                            egui::Button::new("+")
                                                .fill(egui::Color32::TRANSPARENT)
                                                .stroke(egui::Stroke::NONE),
                                        )
                                        .on_hover_text("New tab in default container")
                                        .clicked()
                                    {
                                        app.open_tab("about:blank", ContainerId::default_container());
                                    }
                                });
                                if let Some(id) = select_id {
                                    app.select_tab(&id);
                                }
                                if let Some(id) = close_id {
                                    app.close_tab(&id);
                                }
                            });

                        // ---- Row 2: navigation toolbar ----
                        egui::Panel::top("toolbar")
                            .frame(
                                egui::Frame::new()
                                    .fill(egui::Color32::WHITE)
                                    .inner_margin(egui::Margin::symmetric(8, 5))
                                    .stroke(egui::Stroke::NONE),
                            )
                            .show_inside(ui, |ui| {
                                let (loading, can_back, can_fwd) = {
                                    let p = page.lock().unwrap();
                                    (p.loading, p.can_go_back, p.can_go_forward)
                                };
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    if let Some(wv) = &webview_nav {
                                        if ui
                                            .add_enabled(can_back, icon_button("←"))
                                            .on_hover_text("Back")
                                            .clicked()
                                        {
                                            let _ = wv.go_back(1);
                                        }
                                        if ui
                                            .add_enabled(can_fwd, icon_button("→"))
                                            .on_hover_text("Forward")
                                            .clicked()
                                        {
                                            let _ = wv.go_forward(1);
                                        }
                                        if ui
                                            .add(if loading { icon_button("✕") } else { icon_button("⟳") })
                                            .on_hover_text(if loading { "Stop" } else { "Reload" })
                                            .clicked()
                                        {
                                            if loading {
                                                wv.evaluate_javascript("window.stop()", |_| {});
                                            } else {
                                                wv.reload();
                                            }
                                        }
                                        if ui
                                            .add(icon_button("⌂"))
                                            .on_hover_text("Home")
                                            .clicked()
                                            && let Ok(home) = Url::parse(&webview_home.clone())
                                        {
                                            wv.load(home);
                                        }
                                    }

                                    ui.add_space(4.0);
                                    // URL bar (pill) with a tracking-protection shield.
                                    // Reserve room for the right-side cluster so the pill
                                    // stops at the buttons instead of stretching under them.
                                    let right_reserve = 104.0; // media toggle + menu + spacing
                                    let pill_chrome = 56.0; // inner margins + shield glyph
                                    let text_width =
                                        (ui.available_width() - right_reserve - pill_chrome)
                                            .max(140.0);
                                    let protected_now = app.adblock_enabled();
                                    let shield = if protected_now { "🛡" } else { "⚠" };
                                    egui::Frame::new()
                                        .fill(hex("#f0f0f4"))
                                        .corner_radius(egui::CornerRadius::same(17))
                                        .inner_margin(egui::Margin::symmetric(10, 6))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                if ui
                                                    .label(
                                                        egui::RichText::new(shield).size(15.0),
                                                    )
                                                    .on_hover_text(if protected_now {
                                                        "Tracking protection ON (click to pause)"
                                                    } else {
                                                        "Tracking protection PAUSED (click to enable)"
                                                    })
                                                    .clicked()
                                                {
                                                    app.set_adblock_enabled(!protected_now);
                                                }
                                                let resp = ui.add(
                                                    egui::TextEdit::singleline(buffer)
                                                        .desired_width(text_width)
                                                        .hint_text("Search or enter address"),
                                                );
                                                // Pressing Enter on a single-line field makes it
                                                // lose focus the same frame, so detect the commit via
                                                // lost_focus, not has_focus.
                                                let enter = resp.lost_focus()
                                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                                if enter {
                                                    let target = match OmniboxState::resolve_input(
                                                        buffer,
                                                    ) {
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
                                                }
                                            });
                                        });

                                    // Right-aligned: media hub toggle + container menu.
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui
                                                .add(icon_button("☰"))
                                                .on_hover_text("Menu")
                                                .clicked()
                                            {
                                                *menu_open = true;
                                            }
                                            if ui
                                                .add(icon_button(if *media_open {
                                                    "◉"
                                                } else {
                                                    "◎"
                                                }))
                                                .on_hover_text("Toggle Media Hub")
                                                .clicked()
                                            {
                                                *media_open = !*media_open;
                                            }
                                            let mc = app.media_hub.len();
                                            if mc > 0 {
                                                ui.label(egui::RichText::new(mc.to_string()).weak());
                                            }
                                        },
                                    );
                                });
                            });

                        // ---- Media hub as right panel, only when open ----
                        if *media_open {
                            egui::Panel::right("media_hub")
                                .default_size(300.0)
                                .frame(
                                    egui::Frame::new()
                                        .fill(hex("#f0f0f4"))
                                        .inner_margin(egui::Margin::same(12)),
                                )
                                .show_inside(ui, |ui| {
                                    ui.heading("Media Sessions");
                                    ui.separator();
                                    if app.media_hub.is_empty() {
                                        ui.label(
                                            egui::RichText::new("No media is playing.")
                                                .weak(),
                                        );
                                    } else {
                                        render_media_sessions(ui, app);
                                    }
                                });
                        }

                        // ---- Central viewport: transparent so the page shows through ----
                        egui::CentralPanel::default()
                            .frame(egui::Frame::new().fill(egui::Color32::TRANSPARENT))
                            .show_inside(ui, |_ui| {});

                        // ---- Overflow menu (real actions, incl. container-scoped new tabs) ----
                        if *menu_open {
                            egui::Window::new("Menu")
                                .collapsible(false)
                                .resizable(false)
                                .anchor(egui::Align2::RIGHT_TOP, [-12.0, 56.0])
                                .show(ui.ctx(), |ui| {
                                    if ui.button("New Tab").clicked() {
                                        app.open_tab("about:blank", ContainerId::default_container());
                                        *menu_open = false;
                                    }
                                    if ui
                                        .button(if *media_open {
                                            "Hide Media Hub"
                                        } else {
                                            "Show Media Hub"
                                        })
                                        .clicked()
                                    {
                                        *media_open = !*media_open;
                                        *menu_open = false;
                                    }
                                    ui.separator();
                                    ui.label(egui::RichText::new("New Container Tab").weak());
                                    for c in app.containers.clone() {
                                        let icon = c.icon.clone();
                                        let name = c.name.clone();
                                        let row = ui.horizontal(|ui| {
                                            ui.label(icon);
                                            ui.button(name).clicked()
                                        });
                                        if row.inner {
                                            app.open_tab("about:blank", c.id.clone());
                                            *menu_open = false;
                                        }
                                    }
                                    ui.separator();
                                    if ui
                                        .button(if app.adblock_enabled() {
                                            "Pause Protection"
                                        } else {
                                            "Enable Protection"
                                        })
                                        .clicked()
                                    {
                                        let on = !app.adblock_enabled();
                                        app.set_adblock_enabled(on);
                                        *menu_open = false;
                                    }
                                    if ui.button("Close").clicked() {
                                        *menu_open = false;
                                    }
                                });
                        }
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

/// Parse a `#rrggbb` string into an egui color.
fn hex(s: &str) -> egui::Color32 {
    let h = s.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    egui::Color32::from_rgb(r, g, b)
}

/// A circular icon button sized like Firefox's toolbar buttons (32px, hit target
/// above 24px so it is comfortable to click).
fn icon_button(glyph: &str) -> egui::Button<'_> {
    egui::Button::new(egui::RichText::new(glyph).size(15.0))
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(16))
        .min_size(egui::vec2(32.0, 32.0))
}

/// Firefox Photon light-theme palette. Ink (#1c1b22) on the chrome grey (#f0f0f4)
/// and on white both clear WCAG AA; the single accent (#0060df) is used only for
/// selection, links, and focus, matching the reference.
fn firefox_visuals() -> egui::Visuals {
    let mut v = egui::Visuals::light();
    let chrome = hex("#f0f0f4");
    let ink = hex("#1c1b22");
    let line = hex("#d7d7db");
    let accent = hex("#0060df");

    v.panel_fill = chrome;
    v.window_fill = egui::Color32::WHITE;
    v.extreme_bg_color = egui::Color32::WHITE;
    v.faint_bg_color = hex("#e0e0e6");
    v.hyperlink_color = accent;
    v.override_text_color = Some(ink);

    v.widgets.noninteractive.bg_fill = chrome;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, line);
    v.widgets.inactive.weak_bg_fill = chrome;
    v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    v.widgets.inactive.corner_radius = egui::CornerRadius::same(6);
    v.widgets.hovered.weak_bg_fill = hex("#e0e0e6");
    v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, ink);
    v.widgets.hovered.corner_radius = egui::CornerRadius::same(6);
    v.widgets.active.weak_bg_fill = accent;
    v.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    v.widgets.active.corner_radius = egui::CornerRadius::same(6);

    v.selection.bg_fill = accent;
    v.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);

    v
}

/// Media session cards shared between the always-open and panel layouts.
fn render_media_sessions(ui: &mut egui::Ui, app: &mut BrowserApp) {
    for session in app.media_hub.sessions.clone() {
        ui.group(|ui| {
            ui.label(
                egui::RichText::new(session.display_title())
                    .strong()
                    .size(14.0),
            );
            ui.label(
                egui::RichText::new(format!("{} • {}", session.origin, session.container_id.0))
                    .weak()
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new(format!(
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
                ))
                .size(12.0),
            );
            ui.horizontal(|ui| {
                if ui.button("⏪").on_hover_text("Back 10s").clicked() {
                    app.dispatch_media_action(
                        Some(session.id.clone()),
                        MediaAction::SkipBackward(10_000),
                    );
                }
                let glyph = if session.is_playing() { "⏸" } else { "▶" };
                if ui.button(glyph).on_hover_text("Play/Pause").clicked() {
                    app.dispatch_media_action(
                        Some(session.id.clone()),
                        MediaAction::TogglePlayPause,
                    );
                }
                if ui.button("⏩").on_hover_text("Forward 10s").clicked() {
                    app.dispatch_media_action(
                        Some(session.id.clone()),
                        MediaAction::SkipForward(10_000),
                    );
                }
                let mute = if session.muted { "🔇" } else { "🔊" };
                if ui.button(mute).on_hover_text("Mute").clicked() {
                    app.dispatch_media_action(Some(session.id.clone()), MediaAction::ToggleMute);
                }
            });
            let mut vol = session.volume * 100.0;
            if ui
                .add(egui::Slider::new(&mut vol, 0.0..=100.0).text("Vol"))
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
