// NOTE: This is a slop-coded experimental module and subject to change.

// Generation of Rust struct definitions from the structs that a WESL package
// marks with `@export`.
//
// Every `@export` struct in a module that WESL loads is exported. WESL only
// loads a module when something refers to one of its items, so a struct in a
// module that nothing uses is not found.
//
// A struct is generated one of two ways:
//
// - If it is blittable (all of its fields can be copied byte-for-byte, its
//   layout is identical in every feature variant of the shader, and so are the
//   layouts of the exported structs inside it) then it derives
//   `bytemuck::Pod`. Its layout is taken from Naga, and explicit padding
//   fields and compile-time assertions make sure that the Rust struct has
//   exactly the layout that the shader sees.
// - Otherwise it is a plain Rust struct. Its field types are taken from the
//   unevaluated syntax tree, so it does not matter if the feature variants of
//   the shader disagree about them (for instance, if a field is a `f16` in one
//   variant and `f32` in another). These structs make no promise about their
//   memory layout.

use super::*;
use syn::parse::*;
use wesl::syntax::*;

/// The name of the attribute that marks a struct for export.
const EXPORT_ATTRIBUTE: &str = "export";

/// The Rust types that vectors and matrices are generated as.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VectorTypes {
    /// Plain arrays, like `[f32; 3]`.
    Array,
    /// The types from the `glam` crate, falling back to arrays for the ones
    /// that `glam` has no equivalent for.
    Glam,
}

/// The arguments that were passed to `include_wesl_structs!`.
pub struct Options {
    /// The path to the shaders, as written by the caller.
    pub path: String,
    /// The Rust types to use in place of WESL items, keyed by the path of the
    /// WESL item relative to the package root.
    pub external_types: HashMap<String, syn::Type>,
    /// The Rust types to use for vectors and matrices.
    pub vector_types: VectorTypes,
}

impl Parse for Options {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let path = input.parse::<syn::LitStr>()?.value();
        let mut result = Self {
            path,
            external_types: HashMap::new(),
            vector_types: VectorTypes::Array,
        };

        while input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
            if input.is_empty() {
                break;
            }

            let key = input.parse::<syn::Ident>()?;
            input.parse::<syn::Token![=]>()?;

            match key.to_string().as_str() {
                "vector_types" => {
                    let value = input.parse::<syn::Ident>()?;
                    result.vector_types = match value.to_string().as_str() {
                        "array" => VectorTypes::Array,
                        "glam" => VectorTypes::Glam,
                        other => {
                            return Err(syn::Error::new(
                                value.span(),
                                format!(
                                    "unknown vector types `{other}` (expected `array` or `glam`)"
                                ),
                            ));
                        }
                    };
                }
                "external_types" => {
                    let content;
                    syn::braced!(content in input);
                    while !content.is_empty() {
                        let wesl_path = content.parse::<syn::LitStr>()?.value();
                        content.parse::<syn::Token![=>]>()?;
                        let rust_type = content.parse::<syn::Type>()?;
                        result
                            .external_types
                            .insert(normalize_item_path(&wesl_path), rust_type);

                        if !content.is_empty() {
                            content.parse::<syn::Token![,]>()?;
                        }
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown option `{other}` (expected `external_types` or `vector_types`)"
                        ),
                    ));
                }
            }
        }

        Ok(result)
    }
}

/// Removes any leading `package::` from the path of a WESL item, so that
/// `package::math::Foo` and `math::Foo` name the same item.
fn normalize_item_path(path: &str) -> String {
    path.strip_prefix("package::").unwrap_or(path).to_string()
}

/// A scalar type that can appear in a host-shareable struct.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Prim {
    /// `bool`
    Bool,
    /// `f16`
    F16,
    /// `f32`
    F32,
    /// `f64`
    F64,
    /// `i32`
    I32,
    /// `i64`
    I64,
    /// `u32`
    U32,
    /// `u64`
    U64,
}

impl Prim {
    /// Converts from a Naga scalar.
    fn from_naga(scalar: Scalar) -> Option<Self> {
        Some(match (scalar.kind, scalar.width) {
            (ScalarKind::Bool, _) => Self::Bool,
            (ScalarKind::Float, 2) => Self::F16,
            (ScalarKind::Float, 4) => Self::F32,
            (ScalarKind::Float, 8) => Self::F64,
            (ScalarKind::Sint, 4) => Self::I32,
            (ScalarKind::Sint, 8) => Self::I64,
            (ScalarKind::Uint, 4) => Self::U32,
            (ScalarKind::Uint, 8) => Self::U64,
            _ => return None,
        })
    }

    /// Parses the name of a WGSL scalar type.
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "bool" => Self::Bool,
            "f16" => Self::F16,
            "f32" => Self::F32,
            "f64" => Self::F64,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "u32" => Self::U32,
            "u64" => Self::U64,
            _ => return None,
        })
    }

    /// Parses the suffix of a predeclared alias such as `vec3f` or `mat4x4h`.
    fn from_suffix(suffix: &str) -> Option<Self> {
        Some(match suffix {
            "f" => Self::F32,
            "h" => Self::F16,
            "i" => Self::I32,
            "u" => Self::U32,
            _ => return None,
        })
    }

    /// The Rust type for this scalar.
    fn tokens(self) -> proc_macro2::TokenStream {
        match self {
            Self::Bool => quote::quote!(bool),
            Self::F16 => quote::quote!(f16),
            Self::F32 => quote::quote!(f32),
            Self::F64 => quote::quote!(f64),
            Self::I32 => quote::quote!(i32),
            Self::I64 => quote::quote!(i64),
            Self::U32 => quote::quote!(u32),
            Self::U64 => quote::quote!(u64),
        }
    }

    /// The prefix that `glam` puts in front of the vector types of this scalar.
    fn glam_vector_prefix(self) -> Option<&'static str> {
        Some(match self {
            Self::Bool => "B",
            Self::F32 => "",
            Self::F64 => "D",
            Self::I32 => "I",
            Self::I64 => "I64",
            Self::U32 => "U",
            Self::U64 => "U64",
            Self::F16 => return None,
        })
    }
}

/// A type in a WESL struct, with all aliases resolved.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Ty {
    /// A scalar.
    Scalar(Prim),
    /// A vector of the given length.
    Vector(u8, Prim),
    /// A matrix with the given number of columns and rows.
    Matrix(u8, u8, Prim),
    /// An array of known length.
    Array(Box<Ty>, u32),
    /// A struct or alias, named by its path from the package root
    /// (`module::submodule::Name`).
    Named(String),
}

impl Ty {
    /// Whether values of this type can be copied byte-for-byte to the GPU.
    /// Nested structs are not considered.
    fn is_blittable(&self) -> bool {
        match self {
            Self::Scalar(Prim::Bool)
            | Self::Vector(_, Prim::Bool)
            | Self::Matrix(_, _, Prim::Bool) => false,
            Self::Scalar(_) | Self::Vector(..) | Self::Matrix(..) | Self::Named(_) => true,
            Self::Array(element, _) => element.is_blittable(),
        }
    }

    /// The paths of all of the structs and aliases mentioned by this type.
    fn named(&self) -> Vec<&str> {
        match self {
            Self::Named(key) => vec![key.as_str()],
            Self::Array(element, _) => element.named(),
            _ => Vec::new(),
        }
    }
}

/// A struct that is `public`, and so is exported.
struct ExportedStruct {
    /// The module that the struct is defined in.
    module: ModulePath,
    /// The unmangled name of the struct.
    name: String,
    /// The name of the struct in the compiled shader.
    mangled: String,
    /// The path of the struct from the package root.
    key: String,
    /// The doc comments on the struct.
    docs: Vec<String>,
    /// The members of the struct.
    members: Vec<ExportedMember>,
}

/// A member of an [`ExportedStruct`].
struct ExportedMember {
    /// The name of the field.
    name: String,
    /// The doc comments on the field.
    docs: Vec<String>,
    /// The type of the field, as it appears in the unevaluated syntax tree.
    ty: TypeExpression,
}

/// The layout of one member of a blittable struct.
#[derive(Clone, Debug, Eq, PartialEq)]
struct PodMember {
    /// The name of the field.
    name: String,
    /// Offset from the start of the struct, in bytes.
    offset: u32,
    /// The size of the field, in bytes.
    size: u32,
    /// The type of the field.
    ty: Ty,
}

/// The layout of a blittable struct in one variant of the shader.
#[derive(Clone, Debug, Eq, PartialEq)]
struct PodLayout {
    /// The size of the struct, in bytes, including trailing padding.
    size: u32,
    /// The members of the struct, in order.
    members: Vec<PodMember>,
}

/// Information about the shader package that all of the conversion code needs.
struct Context<'a> {
    /// The unevaluated compilation.
    compilation: &'a CompileResult,
    /// From the mangled name of an item to its module and original name.
    items: HashMap<String, (ModulePath, String)>,
    /// The modules of structs and aliases whose names are not mangled, such as
    /// those that the main module re-exports with `public import`.
    unmangled: HashMap<String, ModulePath>,
    /// All type aliases, by their mangled name.
    aliases: HashMap<String, Vec<&'a TypeAlias>>,
    /// The caller's options.
    options: &'a Options,
}

/// Parses the macro arguments and produces the generated code.
/// Also returns the files that the output depends on.
pub fn generate(shader_path: &Path, options: &Options) -> (proc_macro2::TokenStream, Vec<PathBuf>) {
    let compilation = compile_unevaluated(shader_path);
    let files = source_files(&compilation);
    let context = Context::new(&compilation, options);
    let exports = context.collect_exports();

    let layouts = if exports.is_empty() {
        HashMap::new()
    } else {
        context.blittable_layouts(shader_path, &exports)
    };

    let mut root = ModuleNode::default();
    for export in &exports {
        let layout = layouts.get(&export.mangled);
        let tokens = match options.external_types.get(&export.key) {
            Some(replacement) => render_replacement(export, replacement, layout),
            None => context.render_struct(export, &exports, layout),
        };
        root.insert(&export.module.components, tokens);
    }

    (root.render(&[]), files)
}

/// Writes the checks for an exported struct that the caller replaced with a
/// type of their own. Nothing is generated for the struct itself.
fn render_replacement(
    export: &ExportedStruct,
    replacement: &syn::Type,
    layout: Option<&PodLayout>,
) -> proc_macro2::TokenStream {
    match layout {
        Some(layout) => {
            let size = layout.size as usize;
            let message = format!(
                "the replacement for `{}` does not have the same size as the WESL struct",
                export.key
            );
            quote::quote! {
                const _: () = {
                    ::core::assert!(::core::mem::size_of::<#replacement>() == #size, #message);
                };
            }
        }
        None => proc_macro2::TokenStream::new(),
    }
}

/// A module in the output, which contains structs and other modules.
#[derive(Default)]
struct ModuleNode {
    /// The generated structs in this module.
    items: Vec<proc_macro2::TokenStream>,
    /// Modules inside of this one.
    children: BTreeMap<String, ModuleNode>,
}

impl ModuleNode {
    /// Adds `item` to the module with the given path.
    fn insert(&mut self, path: &[String], item: proc_macro2::TokenStream) {
        match path.split_first() {
            None => self.items.push(item),
            Some((first, rest)) => self
                .children
                .entry(first.clone())
                .or_default()
                .insert(rest, item),
        }
    }

    /// Writes the contents of this module, including child modules.
    fn render(&self, path: &[String]) -> proc_macro2::TokenStream {
        let items = &self.items;
        let children = self.children.iter().map(|(name, child)| {
            let mut child_path = path.to_vec();
            child_path.push(name.clone());
            let doc = format!("Items from the WESL module `{}`.", child_path.join("::"));
            let ident = rust_ident(name);
            let contents = child.render(&child_path);
            quote::quote! {
                #[doc = #doc]
                #[allow(missing_docs, dead_code, clippy::missing_docs_in_private_items)]
                pub mod #ident {
                    #contents
                }
            }
        });

        quote::quote! {
            #(#items)*
            #(#children)*
        }
    }
}

impl<'a> Context<'a> {
    /// Gathers the information used to resolve names.
    fn new(compilation: &'a CompileResult, options: &'a Options) -> Self {
        let items: HashMap<_, _> = compilation
            .sourcemap()
            .expect("source map not generated")
            .items()
            .map(|(mangled, entry)| {
                (
                    mangled.to_string(),
                    (entry.path.clone(), entry.name.clone()),
                )
            })
            .collect();

        let mut unmangled = HashMap::new();
        let mut aliases = HashMap::<String, Vec<&TypeAlias>>::new();
        for module in compilation.modules() {
            for declaration in &module.syntax.global_declarations {
                if let GlobalDeclaration::Struct(Struct { ident, .. })
                | GlobalDeclaration::TypeAlias(TypeAlias { ident, .. }) = &**declaration
                {
                    let name = ident.name().to_string();
                    if !items.contains_key(&name) {
                        unmangled.insert(name, module.path.clone());
                    }
                }

                if let GlobalDeclaration::TypeAlias(alias) = &**declaration {
                    aliases
                        .entry(alias.ident.name().to_string())
                        .or_default()
                        .push(alias);
                }
            }
        }

        Self {
            compilation,
            items,
            unmangled,
            aliases,
            options,
        }
    }

    /// Finds the path from the package root of the item with the
    /// given mangled name.
    fn item_key(&self, mangled: &str) -> Option<String> {
        let (module, name) = self.unmangle(mangled)?;
        Some(item_key(&module, &name))
    }

    /// Finds the module and name that the item called `mangled` was declared
    /// with. Returns `None` for names that the compiler did not mangle, like
    /// predeclared types.
    fn unmangle(&self, mangled: &str) -> Option<(ModulePath, String)> {
        if !self.items.contains_key(mangled) {
            // Items that the main module re-exports, and items in the main
            // module, keep their names.
            return self
                .unmangled
                .get(mangled)
                .map(|module| (module.clone(), mangled.to_string()));
        }

        let (module, name) = wesl::mangler::EscapeMangler.unmangle(mangled)?;

        // Declarations that share a name (as with conditionally-declared
        // aliases) are mangled a second time to tell them apart.
        match self.unmangle(&name) {
            Some(inner) => Some(inner),
            None => Some((module, name)),
        }
    }

    /// The source text of `module`.
    fn source(&self, module: &ModulePath) -> &str {
        self.compilation
            .sourcemap()
            .and_then(|map| map.file(module))
            .map(|file| file.source.as_str())
            .unwrap_or("")
    }

    /// Finds every struct marked `@export`, and checks that the attribute is
    /// only used on structs. WESL only loads modules that something refers to,
    /// so a struct in a module that nothing uses is not found.
    fn collect_exports(&self) -> Vec<ExportedStruct> {
        let mut result = Vec::new();

        for module in self.compilation.modules() {
            let source = self.source(&module.path);

            for declaration in &module.syntax.global_declarations {
                if let GlobalDeclaration::Struct(item) = &**declaration {
                    if has_export_attribute(&item.attributes) {
                        result.push(self.export_struct(
                            &module.path,
                            source,
                            declaration.span().range().start,
                            item,
                        ));
                    }
                } else {
                    let (attributes, kind) = match &**declaration {
                        GlobalDeclaration::Declaration(item) => (&item.attributes, "declaration"),
                        GlobalDeclaration::TypeAlias(item) => (&item.attributes, "type alias"),
                        GlobalDeclaration::Function(item) => (&item.attributes, "function"),
                        _ => continue,
                    };

                    if has_export_attribute(attributes) {
                        fail(format!(
                            "error: `@{EXPORT_ATTRIBUTE}` can only be applied to structs, but it is on a {kind} in module `{}`",
                            module_display(&module.path)
                        ));
                    }
                }
            }
        }

        // Anything left over is on a struct member, function parameter, etc.
        let total = wesl::pass::Visit::<Attributes>::visit(self.compilation.syntax())
            .flatten()
            .filter(|attribute| is_export(attribute))
            .count();
        if total != result.len() {
            fail(format!(
                "error: `@{EXPORT_ATTRIBUTE}` can only be applied to structs (found {} on something else)",
                total.saturating_sub(result.len())
            ));
        }

        result.sort_by(|a, b| a.key.cmp(&b.key));
        result
    }

    /// Gets the name that `ident` had before mangling.
    fn original_name(&self, ident: &str) -> String {
        match self.unmangle(ident) {
            Some((_, name)) => name,
            None => ident.to_string(),
        }
    }

    /// Collects the information about one exported struct.
    fn export_struct(
        &self,
        module: &ModulePath,
        source: &str,
        start: usize,
        item: &Struct,
    ) -> ExportedStruct {
        let mangled = item.ident.name().to_string();
        let name = self.original_name(&mangled);
        let key = item_key(module, &name);

        let mut members = Vec::new();
        for member in &item.members {
            if member.attributes.iter().any(is_conditional) {
                fail_in(
                    &key,
                    format!(
                        "member `{}` uses conditional translation, which is not supported",
                        member.ident.name()
                    ),
                );
            }

            members.push(ExportedMember {
                name: member.ident.name().to_string(),
                docs: doc_comments(source, member.span().range().start),
                ty: member.ty.clone(),
            });
        }

        if item.attributes.iter().any(is_conditional) {
            fail_in(
                &key,
                "conditional translation on exported structs is not supported",
            );
        }

        ExportedStruct {
            module: module.clone(),
            name,
            mangled,
            key,
            docs: doc_comments(source, start),
            members,
        }
    }

    /// Determines what Rust type a WESL type expression, which is a field of
    /// the exported struct `owner`, refers to.
    fn tree_type(&self, owner: &str, expression: &TypeExpression) -> Ty {
        let name = expression.ident.name().to_string();
        let name = name.as_str();

        if let Some(key) = self.item_key(name) {
            if self.options.external_types.contains_key(&key) {
                return Ty::Named(key);
            }

            if let Some(aliases) = self.aliases.get(name) {
                return match aliases.as_slice() {
                    [alias] if !alias.attributes.iter().any(is_conditional) => {
                        self.tree_type(owner, &alias.ty)
                    }
                    _ => fail_in(
                        owner,
                        format!(
                            "type alias `{key}` is declared conditionally, so it has no single Rust type; add `\"{key}\" => <rust type>` to the `external_types` option"
                        ),
                    ),
                };
            }

            return Ty::Named(key);
        }

        let args = expression.template_args.as_deref().unwrap_or(&[]);
        let arg_type = |index: usize| match args.get(index).map(|arg| &*arg.expression) {
            Some(wesl::syntax::Expression::TypeOrIdentifier(inner)) => self.tree_type(owner, inner),
            _ => fail_in(owner, format!("`{name}` expects a type argument")),
        };
        let scalar_arg = || match arg_type(0) {
            Ty::Scalar(prim) => prim,
            _ => fail_in(owner, format!("`{name}` must have a scalar element type")),
        };

        if let Some(prim) = Prim::from_name(name) {
            return Ty::Scalar(prim);
        }

        if let Some((size, prim)) = parse_vector_alias(name) {
            return Ty::Vector(size, prim);
        }

        if let Some((columns, rows, prim)) = parse_matrix_alias(name) {
            return Ty::Matrix(columns, rows, prim);
        }

        if let Some(size) = name.strip_prefix("vec").and_then(|n| n.parse::<u8>().ok()) {
            return Ty::Vector(size, scalar_arg());
        }

        if let Some((columns, rows)) = name.strip_prefix("mat").and_then(|dimensions| {
            let (columns, rows) = dimensions.split_once('x')?;
            Some((columns.parse::<u8>().ok()?, rows.parse::<u8>().ok()?))
        }) {
            return Ty::Matrix(columns, rows, scalar_arg());
        }

        if name == "array" {
            let element = arg_type(0);
            let length = match args.get(1).map(|arg| &*arg.expression) {
                Some(wesl::syntax::Expression::Literal(LiteralExpression::AbstractInt(n))) => {
                    *n as u32
                }
                Some(wesl::syntax::Expression::Literal(LiteralExpression::U32(n))) => *n,
                Some(wesl::syntax::Expression::Literal(LiteralExpression::I32(n))) => *n as u32,
                _ => fail_in(
                    owner,
                    "array lengths in structs that are not blittable must be integer literals (runtime-sized arrays cannot be exported)",
                ),
            };
            return Ty::Array(Box::new(element), length);
        }

        fail_in(owner, format!("the type `{name}` has no Rust equivalent"))
    }

    /// Determines the Rust type that Naga's `ty` corresponds to. Returns `None`
    /// for types that cannot be exported.
    fn naga_type(&self, module: &Module, ty: Handle<Type>) -> Option<Ty> {
        match &module.types[ty].inner {
            TypeInner::Scalar(scalar) => Some(Ty::Scalar(Prim::from_naga(*scalar)?)),
            TypeInner::Vector { size, scalar } => {
                Some(Ty::Vector(*size as u8, Prim::from_naga(*scalar)?))
            }
            TypeInner::Matrix {
                columns,
                rows,
                scalar,
            } => Some(Ty::Matrix(
                *columns as u8,
                *rows as u8,
                Prim::from_naga(*scalar)?,
            )),
            TypeInner::Array {
                base,
                size: ArraySize::Constant(length),
                ..
            } => Some(Ty::Array(
                Box::new(self.naga_type(module, *base)?),
                length.get(),
            )),
            TypeInner::Struct { .. } => {
                let name = module.types[ty].name.as_deref().unwrap_or_default();
                self.item_key(name).map(Ty::Named)
            }
            _ => None,
        }
    }

    /// Finds the exported structs that can be copied byte-for-byte to the GPU,
    /// and gets their layouts. A struct qualifies if
    ///
    /// - all of its fields are blittable (so no `bool`, for instance),
    /// - it has the same layout in every combination of shader features where
    ///   it is defined, and
    /// - all of the exported structs it contains qualify.
    ///
    /// A struct does not have to be defined in every variant of the shader (for
    /// instance, its module might only be imported when a feature is enabled).
    /// It must be defined in at least one, though.
    ///
    /// The result is keyed by the name of the struct in the compiled shader.
    fn blittable_layouts(
        &self,
        shader_path: &Path,
        exports: &[ExportedStruct],
    ) -> HashMap<String, PodLayout> {
        let features = collect_features(self.compilation);
        let mut result: HashMap<String, PodLayout> = HashMap::new();
        let mut disqualified = HashSet::new();
        let mut defined = HashSet::new();

        for mask in 0..(1u32 << features.len()) {
            let module = compile_naga(&compile_variant(
                shader_path,
                variant_flags(mask, &features),
            ));
            let mut layouter = naga::proc::Layouter::default();
            if let Err(e) = layouter.update(module.to_ctx()) {
                fail(format!("error: failed to compute layouts: {e}"));
            }

            for export in exports {
                let Some(handle) = find_struct(&module, &export.mangled) else {
                    continue;
                };
                defined.insert(export.mangled.as_str());

                match self.struct_layout(&module, &layouter, handle) {
                    Some(layout) => match result.get(&export.mangled) {
                        None => {
                            result.insert(export.mangled.clone(), layout);
                        }
                        Some(first) if *first != layout => {
                            disqualified.insert(export.mangled.clone());
                        }
                        Some(_) => {}
                    },
                    None => {
                        disqualified.insert(export.mangled.clone());
                    }
                }
            }
        }

        if let Some(export) = exports
            .iter()
            .find(|export| !defined.contains(export.mangled.as_str()))
        {
            fail_in(
                &export.key,
                "the struct was not found in any variant of the compiled shader",
            );
        }

        result.retain(|name, _| !disqualified.contains(name));

        // Structs can only be blittable if the exported structs inside of them are.
        loop {
            let blittable = exports
                .iter()
                .filter(|export| result.contains_key(&export.mangled))
                .map(|export| export.key.as_str())
                .collect::<HashSet<_>>();
            let before = result.len();

            result.retain(|_, layout| {
                layout.members.iter().all(|member| {
                    member.ty.named().into_iter().all(|key| {
                        blittable.contains(key)
                            || self.options.external_types.contains_key(key)
                            || !exports.iter().any(|export| export.key == key)
                    })
                })
            });

            if result.len() == before {
                break;
            }
        }

        result
    }

    /// Reads the layout of an exported struct out of a compiled shader.
    /// `handle` is the struct's type, from [`find_struct`].
    /// Returns `None` if the struct cannot be copied byte-for-byte.
    fn struct_layout(
        &self,
        module: &Module,
        layouter: &naga::proc::Layouter,
        handle: Handle<Type>,
    ) -> Option<PodLayout> {
        let TypeInner::Struct { members, span } = &module.types[handle].inner else {
            unreachable!("filtered by find_struct");
        };

        let mut result = Vec::new();
        for member in members {
            let ty = self.naga_type(module, member.ty)?;
            if !ty.is_blittable() {
                return None;
            }

            result.push(PodMember {
                name: member.name.clone().unwrap_or_default(),
                offset: member.offset,
                size: layouter[member.ty].size,
                ty,
            });
        }

        Some(PodLayout {
            size: *span,
            members: result,
        })
    }

    /// Converts a field type of the exported struct `export` into Rust code.
    /// `pod` is whether the struct is blittable.
    fn render_type(
        &self,
        export: &ExportedStruct,
        ty: &Ty,
        exports: &[ExportedStruct],
        pod: bool,
    ) -> proc_macro2::TokenStream {
        let glam_name = match ty {
            Ty::Vector(size, prim) if self.options.vector_types == VectorTypes::Glam => prim
                .glam_vector_prefix()
                .map(|prefix| format!("{prefix}Vec{size}")),
            Ty::Matrix(columns, rows, prim) if self.options.vector_types == VectorTypes::Glam => {
                glam_matrix(*columns, *rows, *prim, pod)
            }
            _ => None,
        };
        if let Some(name) = glam_name {
            let name = quote::format_ident!("{name}");
            return quote::quote!(::include_wesl::glam::#name);
        }

        match ty {
            Ty::Scalar(prim) => prim.tokens(),
            Ty::Vector(size, prim) => {
                let prim = prim.tokens();
                let size = *size as usize;
                quote::quote!([#prim; #size])
            }
            Ty::Matrix(columns, rows, prim) => {
                // Matrices are arrays of columns. In memory, WGSL pads the
                // columns of three-row matrices to the size of four rows, so
                // blittable structs expose the padding element.
                let rows = if pod && *rows == 3 { 4 } else { *rows } as usize;
                let columns = *columns as usize;
                let prim = prim.tokens();
                quote::quote!([[#prim; #rows]; #columns])
            }
            Ty::Array(element, length) => {
                let element = self.render_type(export, element, exports, pod);
                let length = *length as usize;
                quote::quote!([#element; #length])
            }
            Ty::Named(key) => {
                if let Some(replacement) = self.options.external_types.get(key) {
                    return quote::quote!(#replacement);
                }

                let Some(target) = exports.iter().find(|export| &export.key == key) else {
                    fail_in(
                        &export.key,
                        format!(
                            "`{key}` is not exported; mark it `@export`, or add an entry for it to the `external_types` option"
                        ),
                    );
                };

                // The shortest path to `target` from the current module.
                let module = &export.module.components;
                let target_module = &target.module.components;
                let shared = module
                    .iter()
                    .zip(target_module)
                    .take_while(|(a, b)| a == b)
                    .count();
                let mut path = vec![if shared == module.len() {
                    quote::format_ident!("self")
                } else {
                    quote::format_ident!("super")
                }];
                path.extend((shared + 1..module.len()).map(|_| quote::format_ident!("super")));
                path.extend(target_module[shared..].iter().map(|c| rust_ident(c)));
                path.push(rust_ident(&target.name));
                quote::quote!(#(#path)::*)
            }
        }
    }

    /// Writes the definition of a struct.
    fn render_struct(
        &self,
        export: &ExportedStruct,
        exports: &[ExportedStruct],
        layout: Option<&PodLayout>,
    ) -> proc_macro2::TokenStream {
        let name = rust_ident(&export.name);
        let docs = export.docs.iter().map(|line| quote::quote!(#[doc = #line]));

        if let Some(layout) = layout {
            let mut fields = Vec::new();
            let mut assertions = Vec::new();
            let mut cursor = 0u32;
            let mut padding = 0;

            let mut push_padding = |fields: &mut Vec<proc_macro2::TokenStream>, bytes: u32| {
                if bytes > 0 {
                    let field = quote::format_ident!("_padding{padding}");
                    let bytes = bytes as usize;
                    fields.push(quote::quote! {
                        #[doc = "Explicit padding, so that the struct matches the shader's layout."]
                        pub #field: [u8; #bytes]
                    });
                    padding += 1;
                }
            };

            for (member, source) in layout.members.iter().zip(&export.members) {
                push_padding(&mut fields, member.offset - cursor);
                cursor = member.offset + member.size;

                let field = rust_ident(&member.name);
                let ty = self.render_type(export, &member.ty, exports, true);
                let docs = source.docs.iter().map(|line| quote::quote!(#[doc = #line]));
                fields.push(quote::quote! {
                    #(#docs)*
                    pub #field: #ty
                });

                let offset = member.offset as usize;
                let size = member.size as usize;
                let message = format!(
                    "field `{}` of `{}` does not match the layout of the WESL struct",
                    member.name, export.key
                );
                assertions.push(quote::quote! {
                    ::core::assert!(::core::mem::offset_of!(#name, #field) == #offset, #message);
                    ::core::assert!(::core::mem::size_of::<#ty>() == #size, #message);
                });
            }
            push_padding(&mut fields, layout.size - cursor);

            let size = layout.size as usize;
            let message = format!(
                "`{}` does not have the same size as the WESL struct",
                export.key
            );

            quote::quote! {
                #(#docs)*
                #[derive(::core::marker::Copy, ::core::clone::Clone, ::core::fmt::Debug, ::include_wesl::bytemuck::Pod, ::include_wesl::bytemuck::Zeroable)]
                #[bytemuck(crate = "::include_wesl::bytemuck")]
                #[repr(C)]
                #[allow(missing_docs, dead_code, clippy::missing_docs_in_private_items)]
                pub struct #name {
                    #(#fields,)*
                }

                impl ::core::default::Default for #name {
                    fn default() -> Self {
                        ::include_wesl::bytemuck::Zeroable::zeroed()
                    }
                }

                const _: () = {
                    ::core::assert!(::core::mem::size_of::<#name>() == #size, #message);
                    #(#assertions)*
                };
            }
        } else {
            let mut fields = Vec::new();
            for member in &export.members {
                let ty = self.tree_type(&export.key, &member.ty);
                let ty = self.render_type(export, &ty, exports, false);
                let field = rust_ident(&member.name);
                let docs = member.docs.iter().map(|line| quote::quote!(#[doc = #line]));
                fields.push(quote::quote! {
                    #(#docs)*
                    pub #field: #ty
                });
            }

            quote::quote! {
                #(#docs)*
                #[derive(::core::marker::Copy, ::core::clone::Clone, ::core::fmt::Debug)]
                #[allow(missing_docs, dead_code, clippy::missing_docs_in_private_items)]
                pub struct #name {
                    #(#fields,)*
                }
            }
        }
    }
}

/// Finds the struct called `mangled` in a compiled shader. Returns `None` if
/// this variant of the shader does not define it.
fn find_struct(module: &Module, mangled: &str) -> Option<Handle<Type>> {
    module
        .types
        .iter()
        .find(|(_, ty)| {
            ty.name.as_deref() == Some(mangled) && matches!(ty.inner, TypeInner::Struct { .. })
        })
        .map(|(handle, _)| handle)
}

/// Stops compilation with an error about the exported struct `owner`.
fn fail_in(owner: &str, message: impl std::fmt::Display) -> ! {
    fail(format!("error in exported struct `{owner}`: {message}"))
}

/// Gets the name of the `glam` type that matches a WGSL matrix, if there is
/// one. In `pod` structs, the type must also have the same layout.
fn glam_matrix(columns: u8, rows: u8, prim: Prim, pod: bool) -> Option<String> {
    let prefix = match prim {
        Prim::F32 => "",
        Prim::F64 => "D",
        _ => return None,
    };

    // `glam`'s `Mat3` has no padding between columns, but WGSL's `mat3x3`
    // pads each column to the alignment of a four-element vector.
    let suffix = match (columns, rows) {
        (3, 3) if pod && prim == Prim::F32 => "A",
        (3, 3) if pod => return None,
        (2, 2) | (3, 3) | (4, 4) => "",
        _ => return None,
    };

    Some(format!("{prefix}Mat{columns}{suffix}"))
}

/// Determines whether `attribute` is `@export`.
fn is_export(attribute: &wesl::syntax::AttributeNode) -> bool {
    matches!(&**attribute, Attribute::Custom(CustomAttribute { name, .. }) if name == EXPORT_ATTRIBUTE)
}

/// Determines whether any of `attributes` is `@export`.
fn has_export_attribute(attributes: &Attributes) -> bool {
    attributes.iter().any(is_export)
}

/// Determines whether `attribute` is part of conditional translation.
fn is_conditional(attribute: &wesl::syntax::AttributeNode) -> bool {
    matches!(
        &**attribute,
        Attribute::If(_) | Attribute::Elif(_) | Attribute::Else
    )
}

/// Gets the path of an item from the package root, such as `math::Vec`.
fn item_key(module: &ModulePath, name: &str) -> String {
    module
        .components
        .iter()
        .map(String::as_str)
        .chain([name])
        .collect::<Vec<_>>()
        .join("::")
}

/// The path of `module` as a string for use in messages.
fn module_display(module: &ModulePath) -> String {
    if module.components.is_empty() {
        "package".to_string()
    } else {
        format!("package::{}", module.components.join("::"))
    }
}

/// Creates a Rust identifier, using the raw form if it would be a keyword.
fn rust_ident(name: &str) -> proc_macro2::Ident {
    if syn::parse_str::<syn::Ident>(name).is_ok() {
        proc_macro2::Ident::new(name, proc_macro2::Span::call_site())
    } else {
        proc_macro2::Ident::new_raw(name, proc_macro2::Span::call_site())
    }
}

/// Parses `vec3f` and the like.
fn parse_vector_alias(name: &str) -> Option<(u8, Prim)> {
    let rest = name.strip_prefix("vec")?;
    let (size, suffix) = rest.split_at(rest.len().checked_sub(1)?);
    Some((size.parse().ok()?, Prim::from_suffix(suffix)?))
}

/// Parses `mat3x4f` and the like.
fn parse_matrix_alias(name: &str) -> Option<(u8, u8, Prim)> {
    let rest = name.strip_prefix("mat")?;
    let (dimensions, suffix) = rest.split_at(rest.len().checked_sub(1)?);
    let (columns, rows) = dimensions.split_once('x')?;
    Some((
        columns.parse().ok()?,
        rows.parse().ok()?,
        Prim::from_suffix(suffix)?,
    ))
}

/// Reads the `///` doc comments that sit immediately above the declaration
/// that starts at byte `start` of `source`. Attribute lines between the
/// comments and the declaration are skipped.
fn doc_comments(source: &str, start: usize) -> Vec<String> {
    let before = source.get(..start).unwrap_or_default();
    let mut lines = before.split('\n').collect::<Vec<_>>();

    // The text on the same line as the start of the declaration.
    lines.pop();

    let mut docs = Vec::new();
    let mut in_docs = false;
    for line in lines.into_iter().rev() {
        let line = line.trim();
        if !in_docs && line.starts_with('@') {
            continue;
        }

        if let Some(doc) = line.strip_prefix("///") {
            in_docs = true;
            docs.push(doc.strip_prefix(' ').unwrap_or(doc).to_string());
        } else {
            break;
        }
    }

    docs.reverse();
    docs
}

/// Tests for the helper functions of the macro.
#[cfg(test)]
mod tests {
    use super::*;

    /// The predeclared aliases are understood.
    #[test]
    fn test_aliases() {
        assert_eq!(parse_vector_alias("vec3f"), Some((3, Prim::F32)));
        assert_eq!(parse_vector_alias("vec2u"), Some((2, Prim::U32)));
        assert_eq!(parse_vector_alias("vec4"), None);
        assert_eq!(parse_matrix_alias("mat4x3h"), Some((4, 3, Prim::F16)));
        assert_eq!(parse_matrix_alias("mat4x3"), None);
    }

    /// By default, vectors and matrices are arrays, and three-row matrices
    /// in blittable structs expose the padding of their columns.
    #[test]
    fn test_arrays_are_used_by_default() {
        let mut options = fixture_options();
        options.vector_types = VectorTypes::Array;
        let output = generate_fixture(&options);

        assert!(!output.contains("glam::Vec"));
        assert!(!output.contains("glam::Mat"));
        assert!(output.contains("pubmin:[f32;3usize]"));
        assert!(output.contains("pubview_projection:[[f32;4usize];4usize]"));
        assert!(output.contains("pubbasis:[[f32;4usize];3usize]"));
        assert!(output.contains("pubids:[u32;2usize]"));
    }

    /// Structs are `Pod` whenever their layout allows it.
    #[test]
    fn test_blittable_structs_derive_pod() {
        let output = generate_package(
            "blittable",
            &[("package.wesl", "@export struct A { x: u32, y: vec2f }")],
            &[],
        );
        assert!(output.contains("Pod"), "{output}");
        assert!(output.contains("size_of :: < A >"), "{output}");
    }

    /// `bool` has no defined layout for the host, so the struct is not `Pod`.
    #[test]
    fn test_bool_makes_structs_plain() {
        let output = generate_package(
            "bool",
            &[("package.wesl", "@export struct A { x: bool }")],
            &[],
        );
        assert!(output.contains("pub x : bool"), "{output}");
        assert!(!output.contains("Pod"), "{output}");
    }

    /// Without an entry for the conditional alias, plain structs cannot be
    /// generated.
    #[test]
    #[should_panic(expected = "type alias `math::m16` is declared conditionally")]
    fn test_conditional_alias_needs_a_type() {
        let mut options = fixture_options();
        options.external_types.remove("math::m16");
        generate_fixture(&options);
    }

    /// Doc comments are carried over.
    #[test]
    fn test_docs_are_generated() {
        let output = generate_fixture(&fixture_options());
        assert!(output.contains("#[doc=\"Sitsintherootmodule.\"]"));
        assert!(output.contains("#[doc=\"Each`vec3f`ispaddedto16bytes.\"]"));
    }

    /// Doc comments are found above attributes, and stop at other text.
    #[test]
    fn test_docs_skip_attributes() {
        let source = "fn x() {}\n/// First\n///Second\n@align(16)\n@size(32)\nstruct A {}";
        let start = source.find("struct").expect("struct in source");
        assert_eq!(doc_comments(source, start), ["First", "Second"]);
    }

    /// Plain comments are not docs, and a blank line ends the docs.
    #[test]
    fn test_docs_stop_at_other_lines() {
        let source = "/// Detached\n\n// comment\nstruct A {}";
        let start = source.find("struct").expect("struct in source");
        assert!(doc_comments(source, start).is_empty());
    }

    /// Attribute text on the same line is not mistaken for docs.
    #[test]
    fn test_docs_with_same_line_attributes() {
        let source = "/// Docs\n@align(16) struct A {}";
        let start = source.find("struct").expect("struct in source");
        assert_eq!(doc_comments(source, start), ["Docs"]);
    }

    /// The attribute only makes sense on structs.
    #[test]
    #[should_panic(expected = "can only be applied to structs, but it is on a function")]
    fn test_export_rejects_functions() {
        generate_package("function", &[("package.wesl", "@export fn f() {}")], &[]);
    }

    /// Members cannot be exported either.
    #[test]
    #[should_panic(expected = "can only be applied to structs (found 1 on something else)")]
    fn test_export_rejects_members() {
        generate_package(
            "member",
            &[("package.wesl", "@export struct A { @export x: u32 }")],
            &[],
        );
    }

    /// Types without a `glam` equivalent are arrays even in `glam` mode.
    #[test]
    fn test_glam_mode_falls_back_to_arrays() {
        let source = "
            enable f16;
            @export struct A { h: vec3h, m: mat2x3f, d: mat3x3<f64>, v: vec4f }
        ";
        let output = generate_package("fallback", &[("package.wesl", source)], &[]);
        assert!(output.contains("pub h : [f16 ; 3usize]"), "{output}");
        assert!(
            output.contains("pub m : [[f32 ; 4usize] ; 2usize]"),
            "{output}"
        );
        assert!(
            output.contains("pub d : [[f64 ; 4usize] ; 3usize]"),
            "{output}"
        );
        assert!(output.contains("include_wesl :: glam :: Vec4"), "{output}");
    }

    /// Fields in the same module are named with `self`.
    #[test]
    fn test_nearby_types_use_short_paths() {
        let output = generate_fixture(&fixture_options());
        assert!(output.contains("pubbounds:self::Bounds"));
    }

    /// Every option can be given, and `package::` is optional in type names.
    #[test]
    fn test_options_all() {
        let options = parse(quote::quote! {
            "shaders",
            vector_types = glam,
            external_types = {
                "package::a::B" => crate::B,
                "c::D" => f32,
            },
        });
        assert_eq!(options.vector_types, VectorTypes::Glam);
        assert_eq!(options.external_types.len(), 2);
        assert!(options.external_types.contains_key("a::B"));
        assert!(options.external_types.contains_key("c::D"));
    }

    /// Only the path is required.
    #[test]
    fn test_options_default() {
        let options = parse(quote::quote!("shaders"));
        assert_eq!(options.path, "shaders");
        assert!(options.external_types.is_empty());
        assert_eq!(options.vector_types, VectorTypes::Array);
    }

    /// The vector types can be arrays or `glam`, and nothing else.
    #[test]
    fn test_options_vector_types() {
        let array = parse(quote::quote!("shaders", vector_types = array));
        assert_eq!(array.vector_types, VectorTypes::Array);
        assert!(syn::parse2::<Options>(quote::quote!("shaders", vector_types = nalgebra)).is_err());
    }

    /// Visibility, and the paths of other crates, are not configurable.
    #[test]
    fn test_options_removed() {
        assert!(syn::parse2::<Options>(quote::quote!("shaders", vis = pub(crate))).is_err());
        assert!(syn::parse2::<Options>(quote::quote!("shaders", glam = ::glam)).is_err());
        assert!(syn::parse2::<Options>(quote::quote!("shaders", bytemuck = ::bytemuck)).is_err());
        assert!(syn::parse2::<Options>(quote::quote!("shaders", types = {})).is_err());
    }

    /// Unknown options are rejected.
    #[test]
    fn test_options_unknown() {
        let result = syn::parse2::<Options>(quote::quote!("shaders", nonsense = 3));
        assert!(result.is_err());
    }

    /// The explicit padding is placed where WGSL's alignment rules need it.
    #[test]
    fn test_padding_is_generated() {
        let output = generate_fixture(&fixture_options());
        assert!(output.contains("pub_padding0:[u8;4usize],#[doc"));
        assert!(output.contains("pubmax:::include_wesl::glam::Vec3"));
        assert!(output.contains("pub_padding1:[u8;4usize]"));
        assert!(output.contains("pubstructRotation"));
        assert!(output.contains("::include_wesl::glam::Mat3A"));
    }

    /// Only structs marked `@export` are exported.
    #[test]
    fn test_private_structs_are_not_exported() {
        let output = generate_package(
            "private",
            &[(
                "package.wesl",
                "struct A { x: u32 }\n@export struct B { x: u32 }",
            )],
            &[],
        );
        assert!(!output.contains("struct A"), "{output}");
        assert!(output.contains("struct B"), "{output}");
    }

    /// Visibility has nothing to do with exporting.
    #[test]
    fn test_public_structs_are_not_exported() {
        let output = generate_package(
            "public",
            &[(
                "package.wesl",
                "public struct A { x: u32 }\n@export struct B { x: u32 }",
            )],
            &[],
        );
        assert!(!output.contains("struct A"), "{output}");
        assert!(output.contains("struct B"), "{output}");
    }

    /// Replacing an exported struct generates a size check, not a struct.
    #[test]
    fn test_replaced_exports_are_only_checked() {
        let output = generate_package(
            "replaced",
            &[("package.wesl", "@export struct A { x: u32, y: u32 }")],
            &[("A", syn::parse_quote!([u32; 2]))],
        );
        assert!(!output.contains("struct A"), "{output}");
        assert!(output.contains("size_of :: < [u32 ; 2] >"), "{output}");
    }

    /// A struct that contains a plain struct cannot be `Pod` either.
    #[test]
    fn test_structs_containing_plain_structs_are_plain() {
        let source = "@export struct A { x: bool }\n@export struct B { a: A, y: u32 }";
        let output = generate_package("nested", &[("package.wesl", source)], &[]);
        assert!(output.contains("struct B"), "{output}");
        assert!(!output.contains("Pod"), "{output}");
    }

    /// Structs are found in other modules that WESL loads because the shader
    /// uses them.
    #[test]
    fn test_structs_in_used_modules_are_found() {
        let output = generate_fixture(&fixture_options());

        assert!(output.contains("pubmodscene{"));
        assert!(output.contains("pubmodmodel{"));
        for name in [
            "Header",
            "Aabb",
            "Bounds",
            "Brick",
            "Immediates",
            "Rotation",
            "Material",
        ] {
            assert!(
                output.contains(&format!("pubstruct{name}{{")),
                "{name} missing"
            );
        }
    }

    /// Fields that are not exported, and not replaced, are reported.
    #[test]
    #[should_panic(expected = "`scene::model::Handle` is not exported")]
    fn test_unexported_field_types_are_reported() {
        let mut options = fixture_options();
        options.external_types.remove("scene::model::Handle");
        generate_fixture(&options);
    }

    /// Modules that the main module does not use are not loaded by WESL, so
    /// their structs are not exported.
    #[test]
    fn test_unimported_modules_are_not_exported() {
        let output = generate_package(
            "unimported",
            &[
                ("package.wesl", "@export struct A { x: u32 }"),
                ("other.wesl", "@export struct B { x: u32 }"),
            ],
            &[],
        );
        assert!(output.contains("struct A"), "{output}");
        assert!(!output.contains("struct B"), "{output}");
    }

    /// A struct that has a different layout in different variants is plain,
    /// and needs the caller to say what the variant-dependent types are.
    #[test]
    fn test_variant_dependent_layout_makes_structs_plain() {
        let source = VARIANT_SOURCE;
        let output = generate_package(
            "variants",
            &[("package.wesl", source)],
            &[("m16", syn::parse_quote!(f32))],
        );
        assert!(output.contains("pub x : f32"), "{output}");
        assert!(!output.contains("Pod"), "{output}");
    }

    /// Without a type for the variant-dependent alias, there is nothing to
    /// generate.
    #[test]
    #[should_panic(expected = "add `\"m16\" => <rust type>` to the `external_types` option")]
    fn test_variant_dependent_layout_needs_a_type() {
        generate_package("variants_error", &[("package.wesl", VARIANT_SOURCE)], &[]);
    }

    /// A struct in a module that is only imported in some variants is still
    /// blittable, as long as its layout agrees wherever it exists.
    #[test]
    fn test_structs_missing_from_some_variants_are_blittable() {
        let output = generate_package(
            "conditional_module",
            &[
                ("package.wesl", CONDITIONAL_MODULE_SOURCE),
                ("sw.wesl", "@export struct B { x: u32, y: f32 }"),
            ],
            &[],
        );
        assert!(output.contains("struct B"), "{output}");
        assert!(output.contains("Pod"), "{output}");
    }

    /// If a struct is missing from some variants but its layout differs in the
    /// ones that have it, then it is plain.
    #[test]
    fn test_conditional_module_with_variant_dependent_layout_is_plain() {
        let output = generate_package(
            "conditional_module_layout",
            &[
                ("package.wesl", CONDITIONAL_MODULE_SOURCE),
                (
                    "sw.wesl",
                    "@if(half) enable f16;
                     @if(half) alias m16 = f16;
                     @if(!half) alias m16 = f32;
                     @export struct B { x: m16 }",
                ),
            ],
            &[("sw::m16", syn::parse_quote!(f32))],
        );
        let (_, module) = output.split_once("pub mod sw").expect("module generated");
        assert!(module.contains("struct B"), "{output}");
        assert!(!module.contains("Pod"), "{output}");
    }

    /// The shaders used by the tests.
    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../include_wesl/tests/structs")
    }

    /// The options used with the fixture.
    fn fixture_options() -> Options {
        let mut options = parse(quote::quote! {
            "structs",
            external_types = {
                "scene::model::Mask" => ::include_wesl::glam::UVec2,
                "scene::model::Handle" => [u32; 2],
                "math::m16" => f32,
            }
        });
        options.vector_types = VectorTypes::Glam;
        options
    }

    /// Generates the code for the fixture, with the whitespace removed to make
    /// matching text easier.
    fn generate_fixture(options: &Options) -> String {
        generate(&fixture(), options).0.to_string().replace(' ', "")
    }

    /// Creates a package in a fresh temporary directory and generates code
    /// for it with the fixture options (apart from the paths).
    fn generate_package(name: &str, files: &[(&str, &str)], types: &[(&str, syn::Type)]) -> String {
        let directory = TempDirectory(std::env::temp_dir().join(format!(
            "include_wesl_structs_{}_{name}",
            std::process::id()
        )));
        let _ = std::fs::remove_dir_all(&directory.0);
        std::fs::create_dir_all(&directory.0).expect("directory should be created");
        for (file, contents) in files {
            std::fs::write(directory.0.join(file), contents).expect("file should be written");
        }

        let mut options = parse(quote::quote!("unused"));
        options.vector_types = VectorTypes::Glam;
        options.external_types.extend(
            types
                .iter()
                .map(|(path, ty)| (path.to_string(), ty.clone())),
        );
        generate(&directory.0, &options).0.to_string()
    }

    /// Parses macro arguments.
    fn parse(tokens: proc_macro2::TokenStream) -> Options {
        syn::parse2(tokens).expect("options should parse")
    }

    /// A directory that is deleted when it goes out of scope.
    struct TempDirectory(PathBuf);

    impl Drop for TempDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The source of a package that only imports the module `sw` when the
    /// `sw` feature is enabled.
    const CONDITIONAL_MODULE_SOURCE: &str = "
        @if(sw) import package::sw;
        @if(sw) fn use_sw(b: sw::B) -> u32 { return 0u; }
        @export struct A { x: u32 }
    ";

    /// The source of a package with a struct that has a different layout in
    /// different variants.
    const VARIANT_SOURCE: &str = "
        @if(half) enable f16;
        @if(half) alias m16 = f16;
        @if(!half) alias m16 = f32;
        @export struct A { x: m16, y: u32 }
    ";
}
