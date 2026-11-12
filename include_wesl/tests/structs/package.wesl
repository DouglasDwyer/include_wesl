import package::scene::model;

/// Sits in the root module.
@export struct Header {
    /// Identifies the format.
    magic: u32,
    flags: u32
}

/// Identifies the format: "MAGI" in ASCII.
@export const HEADER_MAGIC: u32 = 0x4d414749u;

@compute
@workgroup_size(1)
fn main() {
    model::touch();
}
