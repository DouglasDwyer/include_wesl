//! Tests for packages where `import` statements are conditional, so that
//! different combinations of features load different modules.

use include_wesl::*;
use naga::*;
use wgpu::*;

/// The package, which imports `hwrt` or `swrt` depending on `RAY`, and has
/// another feature (`INNER`) that only `hwrt` mentions.
const PACKAGE: WeslPackage = include_wesl!("imports");

/// Determines whether any function in the module uses the `u32` literal
/// `value`.
fn has_literal(module: &Module, value: u32) -> bool {
    module
        .functions
        .iter()
        .map(|(_, function)| function)
        .chain(module.entry_points.iter().map(|entry| &entry.function))
        .flat_map(|function| function.expressions.iter())
        .any(|(_, expression)| *expression == Expression::Literal(Literal::U32(value)))
}

/// A feature that only appears on an `import` is found, and so is one that
/// only appears in a module that only some combinations of features load.
#[test]
fn features_on_imports_and_in_conditionally_imported_modules_are_found() {
    assert_eq!(PACKAGE.features(), ["INNER", "RAY"]);
}

/// Each combination of features loads the module that its `import` selects.
#[test]
fn every_combination_loads_the_right_modules() {
    for mask in 0..4 {
        let [inner, ray] = [0, 1].map(|bit| mask & (1 << bit) != 0);
        let ShaderSource::Naga(module) = PACKAGE.get_source(&[("INNER", inner), ("RAY", ray)])
        else {
            panic!("expected a Naga module");
        };

        let expected = match (ray, inner) {
            (false, _) => 20,
            (true, true) => 11,
            (true, false) => 12,
        };
        assert!(has_literal(&module, expected));
        assert_eq!(
            [11, 12, 20]
                .iter()
                .filter(|v| has_literal(&module, **v))
                .count(),
            1
        );
    }
}
