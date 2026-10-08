# include_wesl

[![Crates.io](https://img.shields.io/crates/v/include_wesl.svg)](https://crates.io/crates/include_wesl)
[![Docs.rs](https://docs.rs/include_wesl/badge.svg)](https://docs.rs/include_wesl)

A tiny proc macro to include a [WESL package](https://wesl-lang.dev) in your binary, and verify that it is valid at compile time.

## Supported functionality

- [Conditional compilation](https://github.com/webgpu-tools/wesl-spec/blob/main/ConditionalTranslation.md)
- [Import statements](https://github.com/webgpu-tools/wesl-spec/blob/main/Imports.md)
- All Naga extensions

## Example

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

See [the examples directory](./examples) for a full demo.

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
        ("RAYTRACING_SUPPORTED", device.features().contains(Feature::EXPERIMENTAL_RAY_QUERY))
    ])
})
```
