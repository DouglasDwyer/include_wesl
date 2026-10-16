//! Tests for the Rust structs that `include_wesl_structs!` generates from the
//! shaders in `structs`, using `glam` types for vectors and matrices.

#![cfg(feature = "glam")]

use core::mem::{offset_of, size_of};
use include_wesl::bytemuck::{bytes_of, cast_slice};
use include_wesl::*;

include_wesl_structs!(
    "structs",
    vector_types = glam,
    external_types = {
        "scene::model::Mask" => ::include_wesl::glam::UVec2,
        "scene::model::Handle" => [u32; 2],
        "math::m16" => f32,
    }
);

/// The same shaders, compiled for use with `wgpu`.
const PACKAGE: WeslPackage = include_wesl!("structs");

/// Exports do not stop the shaders from compiling. The main module uses the
/// model module, so the module with the `half` feature is part of the package.
#[test]
fn exports_do_not_break_compilation() {
    assert_eq!(PACKAGE.features(), ["half"]);

    for half in [false, true] {
        assert!(matches!(
            PACKAGE.get_source(&[("half", half)]),
            wgpu::ShaderSource::Naga(_)
        ));
    }
}

/// The struct in the root module is at the top level.
#[test]
fn root_structs_are_at_top_level() {
    assert_eq!(size_of::<Header>(), 8);
    let header = Header {
        magic: 5,
        ..Default::default()
    };
    assert_eq!(bytes_of(&header), [5, 0, 0, 0, 0, 0, 0, 0]);
}

/// Structs are in modules that match the WESL ones.
#[test]
fn structs_are_nested_in_modules() {
    assert_eq!(size_of::<scene::model::Aabb>(), 32);
    assert_eq!(size_of::<scene::model::Bounds>(), 12);
}

/// `vec3f` is padded out to 16 bytes in WGSL, so there is a gap that has to
/// be filled explicitly.
#[test]
fn padding_matches_wgsl() {
    use scene::model::Aabb;

    assert_eq!(offset_of!(Aabb, max), 16);

    let aabb = Aabb {
        min: ::include_wesl::glam::Vec3::new(1.0, 2.0, 3.0),
        max: ::include_wesl::glam::Vec3::new(4.0, 5.0, 6.0),
        ..Default::default()
    };
    let words = cast_slice::<u8, f32>(bytes_of(&aabb));
    assert_eq!(words, [1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0]);
}

/// A scalar after a vec3 fits in the vector's padding, but the struct is still
/// rounded up to a multiple of 16.
#[test]
fn trailing_padding_matches_wgsl() {
    use scene::model::Immediates;

    assert_eq!(offset_of!(Immediates, camera_position), 64);
    assert_eq!(size_of::<Immediates>(), 80);
}

/// The columns of a `mat3x3f` are padded, which `glam::Mat3A` also does.
#[test]
fn mat3_uses_padded_type() {
    use scene::model::Rotation;

    assert_eq!(offset_of!(Rotation, scale), 48);
    assert_eq!(size_of::<Rotation>(), 64);
}

/// Nested structs refer to each other, and overrides replace fields.
#[test]
fn nested_structs_and_overrides() {
    use scene::model::{Bounds, Brick};

    let brick = Brick {
        bounds: Bounds { inner: [1, 2, 3] },
        data_offset: 4,
        surface: ::include_wesl::glam::UVec2::new(5, 6),
        handles: [[7, 8], [9, 10]],
    };
    let words = cast_slice::<u8, u32>(bytes_of(&brick));
    assert_eq!(words, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
}

/// Structs that are not blittable are plain structs, and can use `bool`
/// and types that differ between variants.
#[test]
fn plain_structs_allow_unshareable_types() {
    use scene::model::{Bounds, Material};

    let material = Material {
        frequency: 0.5_f32,
        ids: ::include_wesl::glam::UVec2::new(1, 2),
        flags: [0; 4],
        transparent: true,
        bounds: Bounds { inner: [0; 3] },
    };
    assert!(material.transparent);
    assert_eq!(material.frequency, 0.5);
}
