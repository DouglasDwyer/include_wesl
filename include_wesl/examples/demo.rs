//! Compiles `foo.wesl` with `include_wesl!`. To see what the macro produces, run
//! `cargo rustc --profile=check --example demo -- -Zunpretty=expanded`.

use include_wesl::*;

/// Relative to this file, like `include_bytes!`.
#[allow(dead_code)]
const PACKAGE: WeslPackage = include_wesl!("foo.wesl");

/// The demo has nothing to run. The interesting part is the expansion of `PACKAGE`.
fn main() { }
