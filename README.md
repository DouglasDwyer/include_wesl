# include_wesl

Compile, check and embed [WESL](https://wesl-lang.dev) shaders when your crate is built.

```rust,ignore
use include_wesl::*;

const SHADERS: WeslPackage = include_wesl!("shaders/package.wesl");

// Pass a value for every feature that the shaders mention...
let source = SHADERS.get_source(&[("high_quality", true), ("debug", false)]);

// ...and create a module from the result.
let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
    label: Some("shaders"),
    source,
});
```

## How it works

`include_wesl!` runs the WESL compiler inside the macro, so a shader with an error is a compile error,
and nothing has to be compiled when the program runs. WESL imports are followed, and each file that is read
is registered with Cargo so that changing it rebuilds the crate. The argument is a string literal for either

* the directory of a WESL package, which must contain a `package.wesl`, or
* the main WESL file, in which case its directory is the root of the package.

A relative path is relative to the file that contains the macro call, like the path of `include_bytes!`,
and an absolute path is used as it is.

### Features

Every name that is used in an `@if` or `@elif` condition of a shader is a
[conditional translation](https://github.com/webgpu-tools/wesl-spec/blob/main/ConditionalTranslation.md)
feature. Features are not tied to anything else, so they can stand for whatever the shaders and the program agree on:
a GPU capability, a quality setting, a debug mode.

Which features are enabled is not known until the program runs, so the macro compiles and validates a module for every
combination of features. `WeslPackage::get_source` picks one when it is called, and only that module is deserialized.
Because the number of modules doubles with every feature, a shader may mention at most 16 of them.

`WeslPackage::features` lists the names that the shaders mention, and `WeslPackage::get_source` needs a value for
each of them.

## Compiler support

A stable compiler of version 1.88 or later works. A nightly compiler is better: it lets the macro tell Cargo about the
shader files directly (`proc_macro_tracked_path`), instead of by including their bytes into an unused constant.
