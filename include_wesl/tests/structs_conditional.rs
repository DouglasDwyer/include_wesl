//! Tests for the structs of packages where `import` statements are
//! conditional.

#![cfg(feature = "structs")]

use core::mem::size_of;
use include_wesl::*;

include_wesl_structs!("imports");

/// Structs are generated for the modules of every combination of features,
/// even though no single combination loads both of them.
#[test]
fn structs_from_all_conditionally_imported_modules_are_generated() {
    assert_eq!(size_of::<hwrt::HwRay>(), 16);
    assert_eq!(size_of::<swrt::SwRay>(), 32);
}
