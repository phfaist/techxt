//! Makes the embedded techxt version available to the binding.
//!
//! `techxt_version()` reports the version of the library that was compiled in, which is
//! what makes a bug report from the web app actionable (web/PLAN.md §6.8). The number
//! lives in `rust/Cargo.toml`'s `[workspace.package]`, and this crate is deliberately
//! not a member of that workspace (§3), so it is read out of the manifest here rather
//! than inherited — three lines of parsing instead of a TOML dependency, for one
//! `version = "…"` line whose shape is fixed by cargo.

fn main() {
    println!("cargo:rerun-if-changed=../../rust/Cargo.toml");
    let manifest = std::fs::read_to_string("../../rust/Cargo.toml").unwrap_or_default();
    let version = manifest
        .lines()
        .skip_while(|l| l.trim() != "[workspace.package]")
        .find_map(|l| l.trim().strip_prefix("version = "))
        .map(|v| v.trim().trim_matches('"').to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=TECHXT_VERSION={version}");
}
