import package::math::m16;

const BRICK_SIZE: u32 = 4u;

/// The number of voxels in a brick. This is evaluated from other constants.
@export const BRICK_VOLUME: u32 = BRICK_SIZE * BRICK_SIZE * BRICK_SIZE;

/// The smallest brick scale.
@export const MIN_SCALE: f32 = 0.25;

/// Whether bricks are compressed.
@export const COMPRESSED: bool = false;

/// An axis-aligned box.
///
/// Each `vec3f` is padded to 16 bytes.
@export struct Aabb {
    /// The lower corner.
    min: vec3f,
    /// The upper corner.
    max: vec3f
}

/// Packed bounds.
@export struct Bounds {
    inner: array<u32, 3>
}

@export struct Brick {
    bounds: Bounds,
    data_offset: u32,
    surface: Mask,
    handles: array<Handle, 2>
}

struct Mask {
    inner: vec2u
}

struct Handle {
    start: u32,
    length: u32
}

@export struct Immediates {
    view_projection: mat4x4f,
    camera_position: vec3f
}

@export struct Rotation {
    basis: mat3x3f,
    scale: f32
}

/// Not blittable, but still useful on the CPU.
@export struct Material {
    /// The percentage of voxels with decorations.
    frequency: m16,
    ids: vec2u,
    flags: array<u32, 4>,
    transparent: bool,
    bounds: Bounds
}

/// Counters that the GPU updates atomically. They are plain integers in Rust.
@export struct Counters {
    head: atomic<u32>,
    tail: atomic<i32>,
    slots: array<atomic<u32>, 2>
}

/// A header followed by a runtime-sized list.
///
/// The list is padded after `count`, because `vec2u` is 8-byte aligned.
@export struct Particles {
    origin: vec4f,
    count: u32,
    /// One entry per particle.
    ids: array<vec2u>
}

alias Handles = array<Handle>;

/// A runtime-sized list of elements that the caller replaced.
@export struct Queue {
    head: atomic<u32>,
    handles: Handles
}

/// Not blittable, so it has no helpers for viewing bytes.
@export struct Labels {
    enabled: bool,
    ids: array<u32>
}

/// Gives the main module something to use, so that this module is loaded.
fn touch() {
}
