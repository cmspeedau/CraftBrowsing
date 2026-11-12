use craft_ui::BrowserApp;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting CraftBrowsing v0.1.0 (Servo + Pure-Rust Browser)");

    let mut app = BrowserApp::new();
    let initial_tab = app.open_tab("craft://welcome", craft_core::container::ContainerId::default_container());

    info!("CraftBrowsing initial session initialized with tab: {:?}", initial_tab);

    Ok(())
}
