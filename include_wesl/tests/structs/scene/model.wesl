import package::math::m16;

/// An axis-aligned box.
///
/// Each `vec3f` is padded to 16 bytes.
@export
struct Aabb {
    /// The lower corner.
    min: vec3f,
    /// The upper corner.
    max: vec3f
}

/// Packed bounds.
@export
struct Bounds {
    inner: array<u32, 3>
}

@export
struct Brick {
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

@export
struct Immediates {
    view_projection: mat4x4f,
    camera_position: vec3f
}

@export
struct Rotation {
    basis: mat3x3f,
    scale: f32
}

/// Not blittable, but still useful on the CPU.
@export
struct Material {
    /// The percentage of voxels with decorations.
    frequency: m16,
    ids: vec2u,
    flags: array<u32, 4>,
    transparent: bool,
    bounds: Bounds
}

/// Gives the main module something to use, so that this module is loaded.
fn touch() {
}
