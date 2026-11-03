//! Stamps each build, so a newer copy of the payload can tell an older one is loaded and take over.

use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    println!("cargo:rustc-env=ERISMENUS_BUILD={now}");
    println!("cargo:rerun-if-changed=src");
}
