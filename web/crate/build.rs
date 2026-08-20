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
