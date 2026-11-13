//! Plugin runtime and WebExtensions sandbox built on `deno_core`.

use deno_core::{JsRuntime, RuntimeOptions};

pub struct PluginRuntime {
    js_runtime: JsRuntime,
}

impl PluginRuntime {
    pub fn new() -> Self {
        let js_runtime = JsRuntime::new(RuntimeOptions::default());
        Self { js_runtime }
    }

    pub fn execute_script(&mut self, script_name: &'static str, code: &str) -> Result<(), String> {
        self.js_runtime
            .execute_script(script_name, code.to_string())
            .map(|_| ())
            .map_err(|e| format!("JavaScript execution error: {e}"))
    }
}
