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

/// Atomics are plain integers in Rust, and keep their place in the layout.
#[test]
fn atomics_become_integers() {
    use scene::model::Counters;

    assert_eq!(size_of::<Counters>(), 16);
    assert_eq!(offset_of!(Counters, tail), 4);
    assert_eq!(offset_of!(Counters, slots), 8);

    let counters = Counters {
        head: 1,
        tail: -2,
        slots: [3, 4],
    };
    assert_eq!(
        bytes_of(&counters),
        [1, 0, 0, 0, 254, 255, 255, 255, 3, 0, 0, 0, 4, 0, 0, 0]
    );
}

/// Views `words` as a dynamically-sized struct whose slice has `count` elements.
macro_rules! unsized_ref {
    ($ty:ty, $words:expr, $count:expr) => {
        // SAFETY: Only the addresses and lengths in the result are inspected,
        // and the words are valid for every field.
        unsafe { &*(core::ptr::slice_from_raw_parts($words.as_ptr(), $count) as *const $ty) }
    };
}

/// The distance in bytes from the start of `value` to the start of `slice`.
fn distance<T: ?Sized, E>(value: &T, slice: &[E]) -> usize {
    slice.as_ptr() as usize - (value as *const T).cast::<u8>() as usize
}

/// A runtime-sized array is a slice at the end of the struct, at the offset
/// that WGSL uses.
#[test]
fn runtime_arrays_are_slices() {
    use scene::model::Particles;

    // `count` ends at byte 20, but `vec2u` is 8-byte aligned, so the array
    // starts at byte 24. The struct is 16-byte aligned because of `origin`.
    #[repr(C, align(16))]
    struct Buffer([u32; 12]);

    assert_eq!(offset_of!(Particles, count), 16);

    let buffer = Buffer([0; 12]);
    let particles = unsized_ref!(Particles, buffer.0, 3);
    assert_eq!(particles.ids.len(), 3);
    assert_eq!(distance(particles, &particles.ids), 24);
    assert_eq!(size_of_val(particles), 48);
    assert_eq!(align_of_val(particles), 16);
}

/// Replaced element types and aliases of unsized arrays are followed.
#[test]
fn runtime_arrays_use_replaced_types() {
    use scene::model::Queue;

    let words = [0u32; 5];
    let queue = unsized_ref!(Queue, words, 2);
    let handles: &[[u32; 2]] = &queue.handles;
    assert_eq!(handles.len(), 2);
    assert_eq!(distance(queue, handles), 4);
    assert_eq!(size_of_val(queue), 20);
}

/// Structs that are not blittable can still end in a slice.
#[test]
fn plain_structs_can_end_in_slices() {
    use scene::model::Labels;

    fn takes_labels(labels: &Labels) -> usize {
        labels.ids.len()
    }
    let _ = takes_labels;
}

/// Constants in the root module are at the top level, and keep their types.
#[test]
fn root_consts_are_at_top_level() {
    assert_eq!(HEADER_MAGIC, 0x4d41_4749);
    assert_eq!(&HEADER_MAGIC.to_le_bytes(), b"IGAM");
}

/// Constants are in modules that match the WESL ones, and their initializers
/// have been evaluated.
#[test]
fn consts_are_evaluated_in_modules() {
    use scene::model::{BRICK_VOLUME, COMPRESSED, MIN_SCALE};

    assert_eq!(BRICK_VOLUME, 64);
    assert_eq!(MIN_SCALE, 0.25);
    const { assert!(!COMPRESSED) };
}

/// Exported constants can be used wherever Rust needs a constant expression.
#[test]
fn consts_are_usable_in_const_contexts() {
    use scene::model::BRICK_VOLUME;

    const WORDS: [u32; BRICK_VOLUME as usize] = [0; BRICK_VOLUME as usize];
    assert_eq!(WORDS.len(), 64);
}
