# include_wesl

[![Crates.io](https://img.shields.io/crates/v/include_wesl.svg)](https://crates.io/crates/include_wesl)
[![Docs.rs](https://docs.rs/include_wesl/badge.svg)](https://docs.rs/include_wesl)

Tiny proc macros to include a [WESL package](https://wesl-lang.dev) in your binary, and verify that it is valid at compile time.

## Supported functionality

- Embedding all variants of a shader using the `include_wesl!` macro
  - [Conditional compilation](https://github.com/webgpu-tools/wesl-spec/blob/main/ConditionalTranslation.md)
  - [Import statements](https://github.com/webgpu-tools/wesl-spec/blob/main/Imports.md)
  - All Naga extensions
- Generating Rust-side structs for WESL types using the `include_wesl_structs!` macro

## Example

### `include_wesl!`

This is how you might create a [`wgpu`](https://github.com/gfx-rs/wgpu) shader module:

```rust,ignore
let shader_package = include_wesl!("shader.wesl");
device.create_shader_module(&ShaderModuleDescriptor {
    label: None,
    source: shader_package.get_source(&[])
})
```

The macro will make sure at compile time that your WESL code is valid using [`naga`](https://crates.io/crates/naga). Otherwise, you will get a friendly error message:

```text
error: no definition in scope for identifier: `bazz`
 --> include_wesl/examples/foo.wesl:6:5
  |
6 | /     @if(some_feature)
7 | |     bazz(workgroup_id.x);
  | |_________________________^ unknown identifier
```

See the examples directory for a full demo.

#### Conditional compilation

[WESL supports annotating sections of code with `@if`, `@elif`, and `@else`](https://github.com/webgpu-tools/wesl-spec/blob/main/ConditionalTranslation.md), similar to the `#[cfg(feature = "")]` syntax in Rust. This crate allows for providing a list of features to enable _at runtime_. This is useful, for example, to enable features only supported on certain GPUs:

```wgsl
@if(RAYTRACING_SUPPORTED)
fn trace_ray(position: vec3f, direction: vec3f) -> vec3f { ... }
```

By providing the `RAYTRACING_SUPPORTED` flag when loading the shader source, it's possible to select a variant of the shader that does (or doesn't) include this method:

```rust,ignore
let shader_package = include_wesl!("shader.wesl");
device.create_shader_module(&ShaderModuleDescriptor {
    label: None,
    source: shader_package.get_source(&[
        ("RAYTRACING_SUPPORTED", device.features().contains(Features::EXPERIMENTAL_RAY_QUERY))
    ])
})
```

### `include_wesl_structs!`

> [!NOTE]
> The `structs` feature flag is required for this functionality.

This is how you might generate bindings for your WESL types in Rust:

```wgsl
// shader.wesl
@export struct Aabb {
    min: vec2f,
    max: vec2f
}
```

```rust,ignore
// main.rs
mod shader {
    include_wesl_structs!("shader.wesl");
}
```

The macro will expand to include a type definition for each `@export` type:

```rust,ignore
// main.rs
mod shader {
    #[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
    #[repr(C)]
    pub struct Aabb {
        pub min: [f32; 2],
        pub max: [f32; 2],
    }

    impl Default for Aabb {
        fn default() -> Self {
            bytemuck::Zeroable::zeroed()
        }
    }

    // Compile-time assertions check that the size and field offsets match the
    // layout of the WESL struct (abbreviated):
    const _: () = {
        assert!(size_of::<Aabb>() == 16);
        assert!(offset_of!(Aabb, min) == 0);
        assert!(offset_of!(Aabb, max) == 8);
        // ...
    };
}
```

The generated bindings will mirror the [package layout](https://github.com/webgpu-tools/wesl-spec/blob/main/Imports.md) of the WESL, with modules and submodules. Any structs that are FFI-compatible will automatically derive `bytemuck::Pod`.

If conditional compilation causes the struct layout to differ between shader variations, a compilation error will be raised:

```wgsl
@if(SHADER_F16) alias m16 = f16;
@if(!SHADER_F16) alias m16 = f32;

@export struct Material {
    frequency: m16
}
```

```text
error in exported struct `Material`: type alias `m16` is declared conditionally,
so it has no single Rust type; add `"m16" => <rust type>` to the `external_types` option
```

#### Customizing the bindings

The `include_wesl_structs!` has the following options:

```rust,ignore
include_wesl_structs!(
    "shaders",
    vector_types = glam,
    external_types = {
        "math::brick::BrickBool" => ::vxcore::math::brick::Brick<bool>,
        "math::m16" => f32,
    }
);
```

- `external_types`: Manually-defined Rust types that should replace their WESL counterparts. Field assertions apply to replaced field types too. Type aliases may also be given a type here.
- `vector_types`: How vectors and matrices are generated. One of the following:
  - `array`: use standard Rust arrays. For instance, `vec3f` becomes `[f32; 3]`.
  - `glam`: use [`glam`](https://crates.io/crates/glam) types. For instance, `vec3f` becomes `Vec3`.
