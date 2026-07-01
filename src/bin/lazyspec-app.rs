//! Entry point for the native macOS desktop shell (RFC-054). A thin wrapper
//! around [`lazyspec::app::run`]; the GUI shell lives in the library so this
//! binary stays a one-liner and the `lazyspec` CLI never links Tauri. Built
//! only under `--features app` (see the `required-features` in `Cargo.toml`).

fn main() -> anyhow::Result<()> {
    lazyspec::app::run()
}
