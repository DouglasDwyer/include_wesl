#![doc = include_str!("../../README.md")]

pub use include_wesl_macros::*;
use std::borrow::*;
use std::collections::*;
use wgpu::*;

/// Holds a compiled shader module (and all of its variants).
/// Allows for instantiating a module at runtime with [`Self::get_source`].
#[derive(Copy, Clone, Debug)]
pub struct WeslPackage {
    /// The list of features referenced by the shader.
    features: &'static [&'static str],
    /// List of serialized [`naga::Module`]s. Each module
    /// corresponds to a variant with some [`Self::features`]
    /// enabled or disabled.
    ///
    /// The list order is determined by the features that are enabled.
    /// For instance, the module at index `0b1001` has `features[0]`
    /// and `features[3]` enabled (because bits 0 and 3 are `true`).
    module_variants: &'static [&'static [u8]],
}

/// Creating packages, and getting the shaders out of them.
impl WeslPackage {
    /// Creates a new package.
    #[doc(hidden)]
    pub const fn new(
        features: &'static [&'static str],
        module_variants: &'static [&'static [u8]],
    ) -> Self {
        assert!(
            module_variants.len() == 1 << features.len(),
            "variant list size not equal to total variant combinations"
        );

        Self {
            features,
            module_variants,
        }
    }

    /// The names of the [conditional translation](https://github.com/webgpu-tools/wesl-spec/blob/main/ConditionalTranslation.md)
    /// features that the shader references, in alphabetical order.
    /// Each one must be passed to [`Self::get_source`].
    pub const fn features(&self) -> &'static [&'static str] {
        self.features
    }

    /// Gets the actual [`ShaderSource`] for use with
    /// [`Device::create_shader_module`].
    ///
    /// The list of `features` determines what gets enabled from [conditional translation](https://github.com/webgpu-tools/wesl-spec/blob/main/ConditionalTranslation.md).
    /// All features referenced by the shader must be provided, both enabled and
    /// disabled. Names that the shader does not reference are ignored.
    ///
    /// # Panics
    ///
    /// Panics if any feature names are omitted, or if the same name appears in
    /// `features` more than once (even if the entries agree, and even if the
    /// shader does not reference the name).
    pub fn get_source(&self, features: &[(&str, bool)]) -> ShaderSource<'static> {
        self.assert_features_unique(features);
        let index = self.module_variant_index(features);
        let (module, _) = bincode::serde::decode_from_slice(
            self.module_variants[index],
            bincode::config::standard(),
        )
        .expect("failed to decode module");
        ShaderSource::Naga(Cow::Owned(module))
    }

    /// Checks that there are no duplicate entries in `features`. Otherwise, panics.
    fn assert_features_unique(&self, features: &[(&str, bool)]) {
        let mut accumulator = HashSet::new();

        for (feature, _) in features {
            if !accumulator.insert(feature) {
                panic!("duplicate feature name `{feature}`");
            }
        }
    }

    /// Determines which module variant to use based upon the enabled features.
    /// Panics if any features are undefined.
    fn module_variant_index(&self, features: &[(&str, bool)]) -> usize {
        let mut result = 0;

        for (i, feature) in self.features.iter().enumerate() {
            let enabled = features
                .iter()
                .find(|(name, _)| name == feature)
                .map(|(_, enabled)| *enabled)
                .unwrap_or_else(|| {
                    panic!("failed to choose shader variant: feature `{feature}` was not provided")
                });

            if enabled {
                result |= 1 << i;
            }
        }

        result
    }
}
