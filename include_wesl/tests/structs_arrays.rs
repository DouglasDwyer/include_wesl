//! Tests for the Rust structs that `include_wesl_structs!` generates from the
//! shaders in `structs`, when vectors and matrices are arrays (the default,
//! even if the `glam` feature is enabled).

#![cfg(feature = "structs")]

use core::mem::{offset_of, size_of};
use include_wesl::bytemuck::{bytes_of, cast_slice};
use include_wesl::*;

include_wesl_structs!(
    "structs",
    external_types = {
        "scene::model::Mask" => [u32; 2],
        "scene::model::Handle" => [u32; 2],
        "math::m16" => f32,
    }
);

/// `vec3f` is padded out to 16 bytes in WGSL, so there is a gap that has to
/// be filled explicitly.
#[test]
fn padding_matches_wgsl() {
    use scene::model::Aabb;

    assert_eq!(offset_of!(Aabb, max), 16);

    let aabb = Aabb {
        min: [1.0, 2.0, 3.0],
        max: [4.0, 5.0, 6.0],
        ..Default::default()
    };
    let words = cast_slice::<u8, f32>(bytes_of(&aabb));
    assert_eq!(words, [1.0, 2.0, 3.0, 0.0, 4.0, 5.0, 6.0, 0.0]);
}

/// The padding in the columns of a `mat3x3f` is part of the array.
#[test]
fn mat3_has_padded_columns() {
    use scene::model::Rotation;

    let rotation = Rotation {
        basis: [
            [1.0, 2.0, 3.0, 0.0],
            [4.0, 5.0, 6.0, 0.0],
            [7.0, 8.0, 9.0, 0.0],
        ],
        scale: 2.0,
        ..Default::default()
    };
    assert_eq!(offset_of!(Rotation, scale), 48);
    assert_eq!(size_of::<Rotation>(), 64);
    assert_eq!(cast_slice::<u8, f32>(bytes_of(&rotation))[12], 2.0);
}

/// Plain structs use arrays too.
#[test]
fn plain_structs_use_arrays() {
    use scene::model::{Bounds, Material};

    let material = Material {
        frequency: 0.5,
        ids: [1, 2],
        flags: [0; 4],
        transparent: true,
        bounds: Bounds { inner: [0; 3] },
    };
    assert_eq!(material.ids, [1, 2]);
}

/// Atomics and runtime-sized arrays are plain integers and slices here too.
#[test]
fn atomics_and_slices_use_arrays() {
    use scene::model::{Counters, Particles};

    let counters = Counters {
        head: 1,
        tail: 2,
        slots: [3, 4],
    };
    assert_eq!(size_of::<Counters>(), 16);
    assert_eq!(counters.slots, [3, 4]);

    #[repr(C, align(16))]
    struct Buffer([u32; 12]);

    let buffer = Buffer([0; 12]);
    // SAFETY: Only the address and length of the result are inspected.
    let particles =
        unsafe { &*(core::ptr::slice_from_raw_parts(buffer.0.as_ptr(), 3) as *const Particles) };
    let ids: &[[u32; 2]] = &particles.ids;
    assert_eq!(ids.len(), 3);
    assert_eq!(
        ids.as_ptr() as usize - (particles as *const Particles).cast::<u8>() as usize,
        24
    );
}
