//! `cargo +nightly fuzz run parse`: `parse_invoice` must never panic, whatever
//! the editor buffer holds. Errors are fine, panics and hangs are not.

#![no_main]

use invgen::config::Config;
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

/// The fictional example config, never the user's real one.
fn config() -> &'static Config {
    static CFG: OnceLock<Config> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut cfg: Config = serde_yaml_ng::from_str(include_str!("../../examples/config.yaml"))
            .expect("examples/config.yaml parses");
        // Auto numbering scans this directory; keep it missing and deterministic.
        cfg.invoices_dir = "/nonexistent/invgen-fuzz".into();
        cfg
    })
}

fuzz_target!(|src: &str| {
    let _ = invgen::parse::parse_invoice(src, config());
});
