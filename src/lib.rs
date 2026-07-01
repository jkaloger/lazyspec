// The `app` module holds the native-shell entry (`run`, Tauri-gated) and the
// Tauri-free protocol-bridge adapter. It compiles for `--features app`, and
// also under the `web`+test build so the bridge adapter's unit tests run
// without pulling Tauri (which the sandbox cannot fetch).
#[cfg(any(feature = "app", all(test, feature = "web")))]
pub mod app;
pub mod cli;
pub mod engine;
pub mod tui;
#[cfg(feature = "web")]
pub mod web;
