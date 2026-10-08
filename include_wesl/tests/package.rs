//! Tests that compile the shaders in `shaders` with the macro, and then use the
//! result.

use include_wesl::*;
use naga::{Expression, Literal, Module};
use wgpu::ShaderSource;

/// Relative to this file, like `include_bytes!`.
const DIRECTORY: WeslPackage = include_wesl!("shaders");

/// The same shaders, but with the main module specified instead of the
/// directory.
const FILE: WeslPackage = include_wesl!("shaders/package.wesl");

/// Usable in constants.
const FEATURES: &[&str] = DIRECTORY.features();

/// Gets the module for a variant of the shaders.
fn module(package: WeslPackage, features: &[(&str, bool)]) -> Module {
    match package.get_source(features) {
        ShaderSource::Naga(module) => module.into_owned(),
        _ => panic!("expected a Naga module"),
    }
}

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

/// A feature that is mentioned several times, in different kinds of places, is
/// listed once.
#[test]
fn features_are_unique_and_sorted() {
    assert_eq!(FEATURES, ["alpha", "beta", "gamma"]);
}

/// Naming the main file is the same as naming its directory.
#[test]
fn file_and_directory_find_the_same_features() {
    assert_eq!(FILE.features(), DIRECTORY.features());
}

/// Each combination of features compiles to a module that has the matching code
/// and no other.
#[test]
fn every_combination_of_features_gets_its_own_module() {
    for package in [DIRECTORY, FILE] {
        for mask in 0..8 {
            let [alpha, beta, gamma] = [0, 1, 2].map(|bit| mask & (1 << bit) != 0);
            let module = module(
                package,
                &[("alpha", alpha), ("beta", beta), ("gamma", gamma)],
            );

            assert!(module.entry_points.iter().any(|entry| entry.name == "main"));
            assert_eq!(
                module
                    .functions
                    .iter()
                    .any(|(_, function)| function.name.as_deref() == Some("alpha_enabled")),
                alpha
            );
            assert_eq!(
                (has_literal(&module, 7), has_literal(&module, 8)),
                (beta, !beta)
            );
            assert_eq!(
                (has_literal(&module, 1000), has_literal(&module, 2000)),
                (gamma, !gamma)
            );
        }
    }
}

/// Extra entries in the features that are passed to `get_source` do not matter.
#[test]
fn unknown_and_repeated_features_are_ignored() {
    let module = module(
        DIRECTORY,
        &[
            ("other", true),
            ("alpha", false),
            ("beta", true),
            ("gamma", true),
            ("other", false),
        ],
    );
    assert!(has_literal(&module, 7));
    assert!(has_literal(&module, 1000));
}

/// There is no default for a feature that is left out.
#[test]
#[should_panic(expected = "feature `beta` was not provided")]
fn omitted_features_panic() {
    let _ = DIRECTORY.get_source(&[("alpha", true), ("gamma", false)]);
}
