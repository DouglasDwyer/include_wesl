#![doc = include_str!("../../README.md")]

#![cfg_attr(nightly, feature(proc_macro_tracked_path))]

use naga::*;
use naga::valid::*;
use proc_macro::*;
use std::collections::*;
use std::path::*;
use wesl::*;
use wesl::sourcemap::*;

/// Compiles the WESL shaders in a directory at build time, and expands to a
/// `WeslPackage` constant expression.
#[proc_macro]
pub fn include_wesl(path: TokenStream) -> TokenStream {
    let file_path = proc_macro::Span::call_site().local_file()
        .expect("source span not associated with file")
        .parent()
        .expect("source file should have parent directory")
        .to_path_buf();

    let requested_path = PathBuf::from(syn::parse_macro_input!(path as syn::LitStr).value());

    let shader_path = if requested_path.is_absolute() {
        requested_path
    }
    else {
        file_path.join(requested_path)
    };

    let unstripped_compilation = compile_wesl(&shader_path, Features { default: Feature::Keep, ..Default::default() });
    let features = collect_features(&unstripped_compilation);
    let variants = compile_variants(&shader_path, &features);
    let serialized_variants = variants.into_iter()
        .map(serialize_module).collect::<Vec<_>>();

    let to_track = track_files(source_files(&unstripped_compilation))
        .parse::<TokenStream>()
        .expect("failed to parse generated tokens");

    
    panic!("tt {to_track}");
}

/// Finds every feature mentioned in an `@if` or `@elif` condition of an unevaluated
/// compilation.
fn collect_features(compilation: &CompileResult) -> Vec<String> {
    use wesl::pass::*;
    use wesl::syntax::*;

    Visit::<Attributes>::visit(compilation.syntax())
        .flatten()
        .filter_map(|attribute| match &**attribute {
            Attribute::If(condition) | Attribute::Elif(condition) => Some(condition),
            _ => None,
        })
        .flat_map(|condition| Visit::<TypeExpression>::visit(&**condition))
        .map(|flag| flag.ident.name().to_string())
        .collect()
}

/// Takes the WGSL output and converts it to a Naga module.
/// The module is validated and the proc macro will fail with an error
/// if there are any issues.
fn compile_naga(compilation: &CompileResult) -> Module {
    let map = compilation.sourcemap()
        .expect("source map not generated");

    let module = naga::front::wgsl::parse_str(compilation.wgsl()).unwrap_or_else(|e| {
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
            fail(map.diagnostic_from_error(e.as_inner(), labels).render_colored())
        });

    module
}

/// Compiles the WESL shader package at `path`.
/// The proc macro will fail with an error if there are any issues.
fn compile_wesl(path: &Path, features: wesl::Features) -> CompileResult {
    let compiler = Compiler::new(CompileOptions {
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
        visibility: true
    });

    match compiler.compile(path) {
        Ok(result) => result,
        Err(e) => fail(e.diagnostic().colored(true)),
    }
}

/// Compiles the shaders at `path` with conditional translation.
/// A variant is generated for every possible combination of `features`.
fn compile_variants(path: &Path, features: &[String]) -> Vec<Module> {
    assert!(features.len() < u32::BITS as usize, "maximum number of conditional features exceeded");

    (0..(1 << features.len()))
        .map(|enabled_mask| compile_naga(&compile_wesl(path, Features { default: Feature::Error, flags: features_xxx(enabled_mask, features) })))
        .collect()
}

/// Prints the contents of `args` to standard error, and then immediately panics.
/// This ensures that `args` are pretty-printed without indentation,
/// since normally the panic message from a proc macro gets extra indented.
fn fail(args: impl std::fmt::Display) -> ! {
    eprintln!("\n{}\n", args);
    panic!();
}

fn features_xxx(enabled_mask: u32, features: &[String]) -> HashMap<String, Feature> {
    let mut result = HashMap::default();

    for (i, name) in features.iter().enumerate() {
        let value = if (enabled_mask & (1 << i)) != 0 {
            Feature::Enable
        }
        else {
            Feature::Disable
        };

        result.insert(name.clone(), value);
    }

    result
}

/// Serializes the provided Naga module with [`bincode`].
fn serialize_module(module: Module) -> Vec<u8> {
    bincode::serde::encode_to_vec(module, bincode::config::standard())
        .expect("failed to encode naga module")
}

/// Gets all files referenced by `compilation`.
/// These are the files that should be tracked for changes.
fn source_files(compilation: &CompileResult) -> Vec<PathBuf> {
    let map = compilation.sourcemap()
        .expect("source map not generated");

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
/// Returns a string that should be included in the macro output.
#[cfg(nightly)]
fn track_files(files: impl IntoIterator<Item = PathBuf>) -> String {
    for file in files {
        proc_macro::tracked::path(file);
    }
    
    String::new()
}

/// Makes Cargo rebuild the caller when one of `files` changes.
/// Returns a string that should be included in the macro output.
#[cfg(not(nightly))]
fn track_files(files: impl IntoIterator<Item = PathBuf>) -> String {
    files
        .into_iter()
        .map(|file| {
            let file = file
                .to_str()
                .expect("shader path was not valid UTF-8");
            format!(
                "const _: &[u8] = include_bytes!({});",
                proc_macro::Literal::string(file)
            )
        })
        .collect()
}