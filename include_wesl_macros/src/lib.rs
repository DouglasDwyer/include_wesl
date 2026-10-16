#![doc = include_str!("../README.md")]
#![cfg_attr(nightly, feature(proc_macro_tracked_path))]

use naga::valid::*;
use naga::*;
use proc_macro::*;
use std::collections::*;
use std::path::*;
use wesl::sourcemap::*;
use wesl::*;

/// Supports generation of Rust struct definitions from WESL.
#[cfg(feature = "structs")]
mod structs;

/// Compiles the WESL shaders at a path at build time, and expands to a
/// `WeslPackage` constant expression.
#[proc_macro]
pub fn include_wesl(path: TokenStream) -> TokenStream {
    let requested_path = syn::parse_macro_input!(path as syn::LitStr).value();
    let shader_path = resolve_path(&requested_path);

    let unstripped_compilation = compile_unevaluated(&shader_path);
    let features = collect_features(&unstripped_compilation);
    let variants = compile_variants(&shader_path, &features);

    let tracked_files = track_files(source_files(&unstripped_compilation));
    let serialized_variants = variants
        .iter()
        .map(|module| proc_macro2::Literal::byte_string(&serialize_module(module)));

    quote::quote! {
        {
            #tracked_files

            ::include_wesl::WeslPackage::new(
                &[ #(#features),* ],
                &[ #(#serialized_variants),* ]
            )
        }
    }
    .into()
}

/// Generates Rust definitions for the WESL structs marked `@export`. See the crate
/// documentation for details.
///
/// The `structs` feature flag is required for this macro.
///
/// # Example
///
/// This is how you might generate bindings for your WESL types in Rust:
///
/// ```wgsl
/// // shader.wesl
/// @export struct Aabb {
///     min: vec2f,
///     max: vec2f
/// }
/// ```
///
/// ```rust,ignore
/// // main.rs
/// mod shader {
///     include_wesl_structs!("shader.wesl");
/// }
/// ```
///
/// The macro will expand to include a type definition for each `@export` type:
///
/// ```rust,ignore
/// // main.rs
/// mod shader {
///     #[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
///     #[repr(C)]
///     pub struct Aabb {
///         pub min: [f32; 2],
///         pub max: [f32; 2],
///     }
///
///     impl Default for Aabb {
///         fn default() -> Self {
///             bytemuck::Zeroable::zeroed()
///         }
///     }
///
///     // Compile-time assertions check that the size and field offsets match the
///     // layout of the WESL struct (abbreviated):
///     const _: () = {
///         assert!(size_of::<Aabb>() == 16);
///         assert!(offset_of!(Aabb, min) == 0);
///         assert!(offset_of!(Aabb, max) == 8);
///         // ...
///     };
/// }
/// ```
#[cfg(feature = "structs")]
#[proc_macro]
pub fn include_wesl_structs(input: TokenStream) -> TokenStream {
    let options = syn::parse_macro_input!(input as structs::Options);
    let shader_path = resolve_path(&options.path);

    if options.vector_types == structs::VectorTypes::Glam && !cfg!(feature = "glam") {
        fail("error: `vector_types = glam` requires the `glam` feature of `include_wesl`");
    }

    let (tokens, files) = structs::generate(&shader_path, &options);
    let tracked_files = track_files(files);

    quote::quote! {
        #tracked_files
        #tokens
    }
    .into()
}

/// Finds every feature mentioned in an `@if` or `@elif` condition of an
/// unevaluated compilation. Each name is listed exactly once.
fn collect_features(compilation: &CompileResult) -> Vec<String> {
    use wesl::pass::*;
    use wesl::syntax::*;

    let mut result = Visit::<Attributes>::visit(compilation.syntax())
        .flatten()
        .filter_map(|attribute| match &**attribute {
            Attribute::If(condition) | Attribute::Elif(condition) => Some(condition),
            _ => None,
        })
        .flat_map(|condition| Visit::<TypeExpression>::visit(&**condition))
        .map(|flag| flag.ident.name().to_string())
        .collect::<Vec<_>>();

    result.sort();
    result.dedup();

    result
}

/// Takes the WGSL output and converts it to a Naga module.
/// The module is validated and the proc macro will fail with an error
/// if there are any issues.
fn compile_naga(compilation: &CompileResult) -> Module {
    let map = compilation.sourcemap().expect("source map not generated");

    // The attributes are replaced by whitespace so that the offsets in the
    // source map stay accurate.
    let wgsl = strip_markers(compilation.wgsl());
    let module = naga::front::wgsl::parse_str(&wgsl).unwrap_or_else(|e| {
        let labels = e
            .labels()
            .filter_map(|(span, msg)| Some((span.to_range()?, msg.to_string())));
        fail(map.diagnostic_from_error(&e, labels).render_colored())
    });

    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .unwrap_or_else(|e| {
            let labels = e
                .spans()
                .filter_map(|(span, msg)| Some((span.to_range()?, msg.clone())));
            fail(
                map.diagnostic_from_error(e.as_inner(), labels)
                    .render_colored(),
            )
        });

    module
}

/// Replaces every `@export` attribute in `wgsl` with spaces. The attribute is
/// only meaningful to `include_wesl_structs!`, and Naga would reject it. The
/// output has the same length as the input.
fn strip_markers(wgsl: &str) -> String {
    let bytes = wgsl.as_bytes();
    let mut result = wgsl.to_string();
    let mut search_from = 0;

    while let Some(offset) = wgsl[search_from..].find('@') {
        let start = search_from + offset;
        let name_start = start + 1;
        let name_end = bytes[name_start..]
            .iter()
            .position(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
            .map_or(bytes.len(), |end| name_start + end);

        if &wgsl[name_start..name_end] == "export" {
            // Both ends are ASCII, so these are character boundaries.
            result.replace_range(start..name_end, &" ".repeat(name_end - start));
        }

        search_from = name_end;
    }

    result
}

/// Compiles the WESL shader package at `path` to WGSL, where the feature flags
/// `flags` are enabled or disabled. All other flags are an error.
/// The proc macro will fail with an error if there are any issues.
fn compile_variant(path: &Path, flags: HashMap<String, Feature>) -> CompileResult {
    compile_wesl(
        path,
        Features {
            default: Feature::Error,
            flags,
        },
    )
}

/// Compiles the WESL shader package at `path`, leaving all conditional
/// translation in place. The result is not valid WGSL, but it can be searched
/// for feature flags. The proc macro will fail with an error if there are any
/// issues.
fn compile_unevaluated(path: &Path) -> CompileResult {
    compile_wesl(
        path,
        Features {
            default: Feature::Keep,
            ..Default::default()
        },
    )
}

/// The options that every compilation in this crate uses, apart from the
/// feature flags.
fn compile_options(features: wesl::Features) -> CompileOptions {
    CompileOptions {
        imports: true,
        condcomp: true,
        constants: Constants::default(),
        dependencies: Vec::new(),
        features,
        generics: false,
        keep: None,
        keep_main: false,
        lower: false,
        mangler: ManglerKind::default(),
        mangle_main: false,
        sort_declarations: false,
        sourcemap: true,
        strip: false,
        validate: false,
        visibility: true,
    }
}

/// Compiles the WESL shader package at `path`. If `lower` is true,
/// WESL-specific syntax is removed so that the output can be parsed as WGSL.
/// The proc macro will fail with an error if there are any issues.
fn compile_wesl(path: &Path, features: wesl::Features) -> CompileResult {
    let compiler = Compiler::new(compile_options(features));

    match compiler.compile(path) {
        Ok(result) => result,
        Err(e) => fail(e.diagnostic().colored(true)),
    }
}

/// Compiles the shaders at `path` with conditional translation.
/// A variant is generated for every possible combination of `features`.
fn compile_variants(path: &Path, features: &[String]) -> Vec<Module> {
    assert!(
        features.len() < u32::BITS as usize,
        "maximum number of conditional features exceeded"
    );

    (0..(1 << features.len()))
        .map(|enabled_mask| {
            compile_naga(&compile_variant(
                path,
                variant_flags(enabled_mask, features),
            ))
        })
        .collect()
}

/// Prints the contents of `args` to standard error, and then immediately
/// panics. This ensures that `args` are pretty-printed without indentation,
/// since normally the panic message from a proc macro gets extra indented.
fn fail(args: impl std::fmt::Display) -> ! {
    // Tests check the message, so it needs to be part of the panic.
    #[cfg(test)]
    panic!("{args}");

    #[cfg(not(test))]
    {
        eprintln!("\n{}\n", args);
        panic!("shader compilation failed");
    }
}

/// Determines the location of the shaders that were requested by the macro
/// caller. Absolute paths are used as-is. Relative paths work like those of
/// [`include_bytes!`]: they are relative to the file that contains the macro
/// call.
fn resolve_path(requested_path: &str) -> PathBuf {
    let requested_path = PathBuf::from(requested_path);

    if requested_path.is_absolute() {
        requested_path
    } else {
        source_directory().join(requested_path)
    }
}

/// Gets the directory that contains the file where the macro was called.
/// The path is relative to the directory that the compiler was started in, if
/// the compiler gave a relative path. That keeps the paths in error messages
/// short.
fn source_directory() -> PathBuf {
    proc_macro::Span::call_site()
        .local_file()
        .expect("source span not associated with file")
        .parent()
        .expect("source file should have parent directory")
        .to_path_buf()
}

/// Serializes the provided Naga module with [`bincode`].
fn serialize_module(module: &Module) -> Vec<u8> {
    bincode::serde::encode_to_vec(module, bincode::config::standard())
        .expect("failed to encode naga module")
}

/// Gets all files referenced by `compilation`.
/// These are the files that should be tracked for changes.
fn source_files(compilation: &CompileResult) -> Vec<PathBuf> {
    let map = compilation.sourcemap().expect("source map not generated");

    let mut files = compilation
        .used_items()
        .iter()
        .filter_map(|(module, _)| map.file(module)?.path.clone())
        .filter_map(|path| path.canonicalize().ok())
        .collect::<Vec<_>>();

    files.sort();
    files.dedup();
    files
}

/// Makes Cargo rebuild the caller when one of `files` changes.
/// Returns tokens that should be included in the macro output.
#[cfg(nightly)]
fn track_files(files: impl IntoIterator<Item = PathBuf>) -> proc_macro2::TokenStream {
    for file in files {
        proc_macro::tracked::path(file);
    }

    proc_macro2::TokenStream::new()
}

/// Makes Cargo rebuild the caller when one of `files` changes.
/// Returns tokens that should be included in the macro output.
#[cfg(not(nightly))]
fn track_files(files: impl IntoIterator<Item = PathBuf>) -> proc_macro2::TokenStream {
    let files = files.into_iter().map(|file| {
        file.to_str()
            .expect("shader path was not valid UTF-8")
            .to_string()
    });

    quote::quote! {
        #(const _: &[u8] = include_bytes!(#files);)*
    }
}

/// Creates the feature flags for a shader variant. The feature at index `i`
/// is enabled if bit `i` of `enabled_mask` is set, and disabled otherwise.
fn variant_flags(enabled_mask: u32, features: &[String]) -> HashMap<String, Feature> {
    features
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let value = if (enabled_mask & (1 << i)) != 0 {
                Feature::Enable
            } else {
                Feature::Disable
            };

            (name.clone(), value)
        })
        .collect()
}

/// Tests for the helper functions of the macro.
#[cfg(test)]
mod tests {
    use super::*;

    /// The shaders used by the integration tests.
    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../include_wesl/tests/shaders")
    }

    /// Bit `i` of the mask enables the feature at index `i`.
    #[test]
    fn variant_flags_enable_features_by_bit() {
        let features = ["a", "b", "c"].map(String::from);
        let flags = variant_flags(0b101, &features);

        assert_eq!(flags.len(), 3);
        assert_eq!(flags["a"], Feature::Enable);
        assert_eq!(flags["b"], Feature::Disable);
        assert_eq!(flags["c"], Feature::Enable);
    }

    /// The attribute for exporting structs is invisible to Naga.
    #[test]
    fn markers_are_replaced_by_whitespace() {
        let source = "@export\nstruct A { @size(8) x: u32 }\n@exported fn f() {} @export";
        let stripped = strip_markers(source);

        assert_eq!(stripped.len(), source.len());
        assert_eq!(
            stripped,
            "       \nstruct A { @size(8) x: u32 }\n@exported fn f() {}        "
        );
    }

    /// A mask has to be able to hold one bit for every feature.
    #[test]
    #[should_panic(expected = "maximum number of conditional features exceeded")]
    fn too_many_features_fail() {
        let features = (0..u32::BITS)
            .map(|i| format!("feature{i}"))
            .collect::<Vec<_>>();
        compile_variants(&fixture(), &features);
    }

    /// A feature that is mentioned several times, in different kinds of places,
    /// is listed once.
    #[test]
    fn features_are_unique_and_sorted() {
        let compilation = compile_unevaluated(&fixture());
        assert_eq!(collect_features(&compilation), ["alpha", "beta", "gamma"]);
    }

    /// Imported files are tracked, along with the main one.
    #[test]
    fn every_file_that_is_used_is_reported() {
        let compilation = compile_unevaluated(&fixture());
        let names = source_files(&compilation)
            .iter()
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .expect("path has a name")
                    .to_string()
            })
            .collect::<Vec<_>>();

        assert_eq!(names, ["math.wesl", "package.wesl"]);
    }

    /// Only relative paths depend on the file that the macro is called from.
    #[test]
    fn absolute_paths_are_not_resolved() {
        let path = if cfg!(windows) {
            "C:\\shaders"
        } else {
            "/shaders"
        };
        assert_eq!(resolve_path(path), Path::new(path));
    }
}
