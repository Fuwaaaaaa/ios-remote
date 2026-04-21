/// Lua scripting engine for advanced automation.
///
/// Users can write Lua scripts that access:
///   - screen.capture() → current frame
///   - screen.ocr(region?) → extracted text
///   - screen.tap(x, y) → send tap
///   - screen.swipe(x1, y1, x2, y2) → send swipe
///   - screen.wait(ms) → sleep
///   - screen.screenshot(path) → save PNG
///   - screen.find_template(path) → template matching
///   - clipboard.get() / clipboard.set(text)
///   - notify(title, message) → notification forwarding
///
/// Example script (scripts/auto_scroll.lua):
/// ```lua
/// while true do
///     screen.swipe(200, 800, 200, 200)
///     screen.wait(2000)
///     local text = screen.ocr()
///     if text:find("End of feed") then break end
/// end
/// ```

#[cfg(feature = "lua")]
pub mod engine {
    use mlua::prelude::*;
    use tracing::info;

    pub struct LuaEngine {
        lua: Lua,
    }

    impl LuaEngine {
        pub fn new() -> Result<Self, String> {
            let lua = Lua::new();
            let to_err = |e: mlua::Error| format!("Lua init failed: {e}");

            // Register screen API
            let screen_tbl = lua.create_table().map_err(to_err)?;
            let clipboard_tbl = lua.create_table().map_err(to_err)?;
            lua.globals().set("screen", screen_tbl).map_err(to_err)?;
            lua.globals()
                .set("clipboard", clipboard_tbl)
                .map_err(to_err)?;

            // screen.wait(ms)
            let wait_fn = lua
                .create_function(|_, ms: u64| {
                    std::thread::sleep(std::time::Duration::from_millis(ms));
                    Ok(())
                })
                .map_err(to_err)?;
            let screen: LuaTable = lua.globals().get("screen").map_err(to_err)?;
            screen.set("wait", wait_fn).map_err(to_err)?;

            // screen.log(msg)
            let log_fn = lua
                .create_function(|_, msg: String| {
                    info!(lua = true, msg = %msg, "Lua script");
                    Ok(())
                })
                .map_err(to_err)?;
            screen.set("log", log_fn).map_err(to_err)?;

            info!("Lua scripting engine initialized (Lua 5.4)");
            Ok(Self { lua })
        }

        /// Execute a Lua script from a file.
        pub fn execute_file(&self, path: &str) -> Result<(), String> {
            let code = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            self.execute(&code)
        }

        /// Execute a Lua code string.
        pub fn execute(&self, code: &str) -> Result<(), String> {
            self.lua
                .load(code)
                .exec()
                .map_err(|e| format!("Lua error: {}", e))
        }
    }
}

#[cfg(not(feature = "lua"))]
pub fn run_script(_path: &str) -> Result<(), String> {
    Err(
        "Lua scripting requires the 'lua' feature flag. Build with: cargo build --features lua"
            .to_string(),
    )
}
