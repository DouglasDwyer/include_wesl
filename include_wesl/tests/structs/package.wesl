import package::scene::model;

/// Sits in the root module.
@export struct Header {
    /// Identifies the format.
    magic: u32,
    flags: u32
}

@compute
@workgroup_size(1)
fn main() {
    model::touch();
}
