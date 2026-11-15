use craft_ui::window::BrowserWindowApp;
use tracing::info;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    info!("Starting CraftBrowsing v0.1.0 (Rust application layer, Servo engine)");

    BrowserWindowApp::run()?;

    Ok(())
}
