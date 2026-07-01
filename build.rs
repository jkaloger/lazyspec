//! Build script. Runs Tauri's codegen only for `--features app`; every other
//! build (default/`web`/`cli`/`tui`) is a no-op so no Tauri build tooling runs.
//! The `tauri-build` crate is itself gated behind the `app` feature, so the
//! `cfg` both selects the codegen and guarantees the crate is present.

fn main() {
    #[cfg(feature = "app")]
    tauri_build::build();
}
