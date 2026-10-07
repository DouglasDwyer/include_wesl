//! Compiles `foo.wesl`, and lists what the result contains.

use include_wesl::*;
use wgpu::*;

/// Relative to this file, like `include_bytes!`.
const PACKAGE: WeslPackage = include_wesl!("foo.wesl");

fn main() {
    println!("features: {:?}", PACKAGE.features());

    // Pass the source to `Device::create_shader_module`.
    for poogie in [false, true] {
        if let ShaderSource::Naga(module) = PACKAGE.get_source(&[("poogie", poogie)]) {
            println!("poogie = {poogie}: {} statement(s) in main", module.entry_points[0].function.body.len());
        }
    }
}
