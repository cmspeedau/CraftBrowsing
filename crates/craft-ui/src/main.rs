use craft_ui::window::BrowserWindowApp;
use tracing::info;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting CraftBrowsing v0.1.0 (Servo + Pure-Rust Browser)");

    BrowserWindowApp::run()?;

    Ok(())
}
