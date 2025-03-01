use std::{
    collections::{HashMap, HashSet},
    time::SystemTime,
};

use camino::{Utf8Path, Utf8PathBuf};
use proc_macro2 as _;
use quote::ToTokens;
use syn::{Attribute, ImplItem, Item, Type};
use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};

const DYLO_RUNTIME_VERSION: &str = "1.0.0";

/// represents a mod crate we're managing, including both its impl & consumer versions.
/// contains paths and timestamps needed for monitoring file changes and determining when
/// regeneration of the consumer version is necessary
#[derive(Debug)]
struct ModInfo {
    /// human-readable name of the mod, extracted from the directory name (without mod- prefix)
    name: String,
    /// location of the mod's implementation code ($workspace/mod-$name/)
    mod_path: Utf8PathBuf,
    /// destination path for generating consumer version ($workspace/$name/)
    con_path: Utf8PathBuf,
    /// timestamp of most recently modified file in mod directory
    mod_timestamp: SystemTime,
    /// timestamp of most recently modified file in consumer directory
    con_timestamp: SystemTime,
}

/// Reason we might have to regenerate a mod's consumer version.
#[derive(Debug)]
enum ProcessReason {
    Force,
    Missing,
    Modified,
}

/// Discover all mods in the `./` directory, recursively
fn list_mods(mods_dir: &camino::Utf8Path) -> std::io::Result<Vec<ModInfo>> {
    let mut mods = Vec::new();
    for entry in walkdir::WalkDir::new(mods_dir) {
        let entry = entry?;
        let mod_path: Utf8PathBuf = entry.path().to_owned().try_into().unwrap();

        if !mod_path.is_dir() {
            continue;
        }

        if !mod_path.join("Cargo.toml").exists() {
            continue;
        }

        let Some(name) = mod_path.file_name().map(|n| n.to_string()) else {
            continue;
        };
        if !name.starts_with("mod-") {
            continue;
        }

        let name = name.trim_start_matches("mod-").to_string();
        let con_path = mod_path.parent().unwrap().join(&name);

        // Check timestamps
        let mod_timestamp = get_latest_timestamp(&mod_path)?;
        let con_timestamp = if con_path.exists() {
            get_latest_timestamp(&con_path)?
        } else {
            SystemTime::UNIX_EPOCH
        };

        mods.push(ModInfo {
            name,
            mod_path,
            con_path,
            mod_timestamp,
            con_timestamp,
        });
    }

    Ok(mods)
}

/// When generating the consumer manifest from a mod manifest:
/// - Changes package name to strip the "mod-" prefix
/// - Removes the dev-dependencies section
/// - Removes the dylo dependency
/// - Removes the "impl" feature & any dependencies it enables
fn prepare_consumer_cargo_file(mod_info: &ModInfo) -> std::io::Result<String> {
    // Parse the TOML doc into an editable format
    let mod_cargo = fs_err::read_to_string(mod_info.mod_path.join("Cargo.toml"))?;
    let mut doc = mod_cargo.parse::<toml_edit::DocumentMut>().unwrap();

    // Update package name to strip the "mod-" prefix
    doc["package"]["name"] = toml_edit::value(mod_info.name.clone());

    // Update crate-type from cdylib to rlib
    let crate_type = doc["lib"]["crate-type"]
        .as_array()
        .expect("lib.crate-type must be an array");

    assert_eq!(
        crate_type.iter().next().unwrap().as_str().unwrap(),
        "cdylib",
        "lib.crate-type must be [\"cdylib\"]"
    );
    doc["lib"]["crate-type"] = toml_edit::value(toml_edit::Array::from_iter(["rlib"]));

    doc["package"]["description"] = toml_edit::value(format!(
        "Consumer module for the mod-{} crate, generated by https://github.com/bearcove/dylo",
        mod_info.name
    ));

    let features_enabled_by_impl_feature: Vec<String> = doc
        .get("features")
        .and_then(|f| f.get("impl"))
        .map(|i| {
            i.as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();

    let mut features_enabled_by_other_features: HashSet<String> = Default::default();
    if let Some(features) = doc.get("features").and_then(|f| f.as_table()) {
        for (name, values) in features.iter() {
            if name == "impl" {
                continue;
            }

            if let Some(array) = values.as_array() {
                for value in array {
                    if let Some(s) = value.as_str() {
                        features_enabled_by_other_features.insert(s.to_string());
                    }
                }
            }
        }
    }

    let mut impl_specific_deps: HashSet<String> = Default::default();

    for f in features_enabled_by_impl_feature.iter() {
        if let Some(stripped) = f.strip_prefix("dep:") {
            impl_specific_deps.insert(stripped.to_string());
        }
    }

    // if a feature is pulled in by impl AND by some other feature,
    // then we need to keep it.
    for f in &features_enabled_by_other_features {
        if let Some(stripped) = f.strip_prefix("dep:") {
            impl_specific_deps.remove(stripped);
        }
    }

    // If there's a features table, update the default features to remove impl feature
    if let Some(features) = doc.get_mut("features") {
        if let Some(default) = features.get_mut("default") {
            let array = default
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v.as_str().unwrap() != "impl")
                .collect::<Vec<_>>();
            *default = toml_edit::value(toml_edit::Array::from_iter(array));
        }
    }

    // Now remove the impl feature altogether
    if let Some(features) = doc.get_mut("features") {
        features.as_table_mut().unwrap().remove("impl");
    }

    // Remove dev-dependencies section if it exists
    if doc.contains_key("dev-dependencies") {
        doc.remove("dev-dependencies");
    }

    // Remove dylo dependency if it exists
    if let Some(deps) = doc.get_mut("dependencies") {
        if deps.is_table() {
            let deps_table = deps.as_table_mut().unwrap();
            // Remove dylo from dependencies
            deps_table.remove("dylo");

            // Add dylo-runtime as a dependency
            deps_table.insert("dylo-runtime", DYLO_RUNTIME_VERSION.into());

            // Remove impl_specific_deps from dependencies
            let mut removed_deps = Vec::new();
            for dep_name in impl_specific_deps {
                if deps_table.contains_key(&dep_name) {
                    removed_deps.push(dep_name.clone());
                    deps_table.remove(&dep_name);
                }
            }
            if !removed_deps.is_empty() {
                tracing::debug!(
                    "Removed {} deps ({})",
                    removed_deps.len(),
                    removed_deps.join(", ")
                );
            }
        }
    }

    Ok(doc.to_string())
}

/// FileSet represents a set of files that need to be generated, stored in memory
/// before being written to disk. This allows checking if any files would actually
/// change before modifying them, which is important because Cargo uses file timestamps
/// to determine what needs to be rebuilt. By only writing files when their contents
/// would change, we avoid triggering unnecessary rebuilds just because timestamps
/// were updated.
///
/// See `-Z checksum-freshness`: <https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#checksum-freshness>
#[derive(Debug, Clone)]
struct FileSet {
    files: HashMap<Utf8PathBuf, String>,
}

impl FileSet {
    fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    /// True if any files are missing from disk or have different contents.
    fn is_different(&self, root: &Utf8Path) -> std::io::Result<bool> {
        for (rel_path, contents) in &self.files {
            let full_path = root.join(rel_path);
            if !full_path.exists() {
                return Ok(true);
            }
            let disk_contents = fs_err::read_to_string(&full_path)?;
            if &disk_contents != contents {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Write file contents to disk, creating parent directories as needed.
    fn commit(&self, root: &Utf8Path) -> std::io::Result<()> {
        for (rel_path, contents) in &self.files {
            let full_path = root.join(rel_path);
            if let Some(parent) = full_path.parent() {
                fs_err::create_dir_all(parent)?;
            }
            fs_err::write(full_path, contents)?;
        }
        Ok(())
    }
}

const SPEC_PATH: &str = ".dylo/spec.rs";
const SUPPORT_PATH: &str = ".dylo/support.rs";

fn process_mod(mod_info: ModInfo, force: bool) -> std::io::Result<()> {
    let mod_ts = mod_info
        .mod_timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let con_ts = mod_info
        .con_timestamp
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let diff = if mod_ts > con_ts {
        format!("mod is newer by {} seconds", mod_ts - con_ts)
    } else {
        format!("con is newer by {} seconds", con_ts - mod_ts)
    };

    tracing::debug!(
        "Mod '{name}' in {mod_path}, {con_path}\n  mod ts = {mod_ts}\n  con ts = {con_ts}\n  {diff}",
        name = mod_info.name,
        mod_path = mod_info.mod_path,
        con_path = mod_info.con_path,
        mod_ts = mod_ts,
        con_ts = con_ts,
        diff = diff
    );

    let reason = if force {
        ProcessReason::Force
    } else if !mod_info.con_path.exists() {
        ProcessReason::Missing
    } else if mod_info.mod_timestamp > mod_info.con_timestamp {
        ProcessReason::Modified
    } else {
        return Ok(());
    };

    tracing::debug!("📦 Processing mod {} (because {:?})", mod_info.name, reason);

    // Generate consumer version by parsing and filtering lib.rs
    let start = std::time::Instant::now();

    let lib_rs = fs_err::read_to_string(mod_info.mod_path.join("src/lib.rs"))?;
    let ast = syn::parse_file(&lib_rs).unwrap();

    let mut con_items: Vec<Item> = ast.items.clone();
    let mut spec_items: Vec<Item> = Default::default();
    transform_ast(&mut con_items, &mut spec_items);

    let duration = start.elapsed();

    let autogen_attrs = vec![syn::parse_quote! {
        #[doc = "This file was automatically generated by https://github.com/bearcove/dylo"]
    }];

    let spec_ast = syn::File {
        shebang: None,
        attrs: autogen_attrs.clone(),
        items: spec_items,
    };
    let spec_formatted = prettyplease::unparse(&spec_ast);

    let mut missing_spec = true;
    let mut missing_support = true;

    for item in &con_items {
        if let Item::Macro(mac) = item {
            if mac.mac.path.is_ident("include") {
                if let Ok(lit) = syn::parse2::<syn::LitStr>(mac.mac.tokens.clone()) {
                    let path = lit.value();
                    if path == SPEC_PATH {
                        missing_spec = false;
                    } else if path == SUPPORT_PATH {
                        missing_support = false;
                    }
                }
            }
        }
    }

    if missing_spec {
        con_items.push(syn::parse_quote! {
            include!(".dylo/spec.rs");
        });
    }

    if missing_support {
        con_items.push(syn::parse_quote! {
            include!(".dylo/support.rs");
        });
    }

    let con_ast = syn::File {
        shebang: None,
        attrs: autogen_attrs.clone(),
        items: con_items,
    };
    let con_formatted = prettyplease::unparse(&con_ast);

    tracing::debug!(
        "📝 Parsed {} in {:.2}s, size: {} bytes",
        mod_info.name,
        duration.as_secs_f32(),
        lib_rs.len()
    );

    // Generate files for mod version
    let mut mod_files = FileSet::new();

    // Check and add "dylo-runtime" dependency to Cargo.toml if needed
    let cargo_toml = fs_err::read_to_string(mod_info.mod_path.join("Cargo.toml"))?;
    let mut doc = cargo_toml.parse::<toml_edit::DocumentMut>().unwrap();

    let mut need_dylo_runtime = true;
    if let Some(deps) = doc.get("dependencies") {
        if deps.is_table() {
            let deps_table = deps.as_table().unwrap();
            if deps_table.contains_key("dylo-runtime") {
                need_dylo_runtime = false;
            }
        }
    }

    if need_dylo_runtime {
        tracing::info!("Adding dylo-runtime dependency to {}", mod_info.name);
        if let Some(deps) = doc.get_mut("dependencies") {
            if deps.is_table() {
                let deps_table = deps.as_table_mut().unwrap();
                deps_table.insert("dylo-runtime", toml_edit::value(DYLO_RUNTIME_VERSION));
                mod_files.files.insert("Cargo.toml".into(), doc.to_string());
            }
        }
    }

    // Add spec.rs to mod version
    mod_files
        .files
        .insert(format!("src/{SPEC_PATH}").into(), spec_formatted.clone());

    let init_src = include_str!("init_template.rs");
    mod_files
        .files
        .insert(format!("src/{SUPPORT_PATH}").into(), init_src.to_string());

    // Check for include statements for spec and support files
    let mut include_paths = HashSet::new();
    for item in &ast.items {
        if let Item::Macro(mac) = item {
            if mac.mac.path.is_ident("include") {
                if let Ok(lit) = syn::parse2::<syn::LitStr>(mac.mac.tokens.clone()) {
                    let path = lit.value();
                    tracing::debug!("the file includes {path:?}");
                    include_paths.insert(path);
                }
            }
        }
    }
    let mut added_suffixes = Vec::new();

    if !include_paths.contains(SPEC_PATH) {
        added_suffixes.push(format!("include!(\"{SPEC_PATH}\");"));
    }
    if !include_paths.contains(SUPPORT_PATH) {
        added_suffixes.push(format!("include!(\"{SUPPORT_PATH}\");"));
    }

    if !added_suffixes.is_empty() {
        let suffix = format!("\n\n{}", added_suffixes.join("\n"));
        let content = format!("{lib_rs}{suffix}");
        mod_files.files.insert("src/lib.rs".into(), content);
    }

    // Generate files for consumer version
    let mut con_files = FileSet::new();

    // Generate Cargo.toml
    let con_cargo = prepare_consumer_cargo_file(&mod_info)?;
    con_files.files.insert("Cargo.toml".into(), con_cargo);

    // Add lib.rs and spec.rs
    con_files.files.insert("src/lib.rs".into(), con_formatted);

    con_files
        .files
        .insert(format!("src/{SPEC_PATH}").into(), spec_formatted);
    let load_src = include_str!("load_template.rs");
    con_files
        .files
        .insert(format!("src/{SUPPORT_PATH}").into(), load_src.to_string());

    // Update mod files if different
    let mod_path = Utf8Path::new(&mod_info.mod_path);
    if mod_files.is_different(mod_path)? {
        tracing::info!("📝 Changes detected in mod files for {}", mod_info.name);
        mod_files.commit(mod_path)?;
    }

    // Update consumer files if different
    let con_path = Utf8Path::new(&mod_info.con_path);
    if con_files.is_different(con_path)? {
        tracing::info!(
            "📝 Changes detected in consumer files for {}",
            mod_info.name
        );
        con_files.commit(con_path)?;

        // Verify compilation
        tracing::info!("🔨 Running cargo check for {}", mod_info.name);
        let start = std::time::Instant::now();
        let status = std::process::Command::new("cargo")
            .arg("check")
            .arg("--package")
            .arg(&mod_info.name)
            .status()?;

        let duration = start.elapsed();
        if status.success() {
            tracing::info!("✅ Check passed in {:.2}s", duration.as_secs_f32());
        } else {
            tracing::error!(
                "❌ Check failed for {} in {:.2}s",
                mod_info.name,
                duration.as_secs_f32()
            );
            if force {
                tracing::warn!("⚠️ Continuing despite failed cargo check due to --force");
            } else {
                tracing::error!("⛔ Exiting due to failed cargo check");
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

fn item_attributes(item: &mut Item) -> Option<&mut Vec<Attribute>> {
    match item {
        Item::Const(item) => Some(&mut item.attrs),
        Item::Enum(item) => Some(&mut item.attrs),
        Item::ExternCrate(item) => Some(&mut item.attrs),
        Item::Fn(item) => Some(&mut item.attrs),
        Item::ForeignMod(item) => Some(&mut item.attrs),
        Item::Impl(item) => Some(&mut item.attrs),
        Item::Macro(item) => Some(&mut item.attrs),
        Item::Mod(item) => Some(&mut item.attrs),
        Item::Static(item) => Some(&mut item.attrs),
        Item::Struct(item) => Some(&mut item.attrs),
        Item::Trait(item) => Some(&mut item.attrs),
        Item::TraitAlias(item) => Some(&mut item.attrs),
        Item::Type(item) => Some(&mut item.attrs),
        Item::Union(item) => Some(&mut item.attrs),
        Item::Use(item) => Some(&mut item.attrs),
        Item::Verbatim(_) => None,
        _ => None,
    }
}

// recognizes `#[cfg(feature = "impl"]`
fn is_cfg_feature_impl(attr: &Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }

    let mut has_feature_impl = false;
    let _ = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("feature") {
            let content = meta.input.to_string();
            if content == "= \"impl\"" {
                has_feature_impl = true;
            }
        }
        Ok(())
    });
    has_feature_impl
}

// recognizes `#[cfg(not(feature = "impl"))]`
fn is_cfg_not_feature_impl(attr: &Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }

    let mut has_not = false;
    let mut has_feature_impl = false;
    let _ = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("not") {
            let _ = meta.parse_nested_meta(|meta| {
                if meta.path.is_ident("feature") {
                    let content = meta.input.to_string();
                    if content == "= \"impl\"" {
                        has_feature_impl = true;
                    }
                }
                Ok(())
            });
            has_not = true;
        }
        Ok(())
    });
    has_not && has_feature_impl
}

fn is_cfg_test(attr: &Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }

    let mut has_test = false;
    let _ = attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("test") {
            has_test = true;
        }
        Ok(())
    });
    has_test
}

fn should_remove_item(item: &mut Item) -> bool {
    if let Some(attrs) = item_attributes(item) {
        for attr in attrs {
            if is_cfg_feature_impl(attr) || is_cfg_test(attr) {
                return true;
            }
        }
    }
    false
}

enum InterfaceType {
    NonSync,
    Sync,
}

impl InterfaceType {
    // see definitions of `waxpoetic` itself
    fn supertraits(&self) -> syn::punctuated::Punctuated<syn::TypeParamBound, syn::Token![+]> {
        let mut p = syn::punctuated::Punctuated::new();
        match self {
            InterfaceType::NonSync => {
                p.push(syn::parse_quote!(Send));
                p.push(syn::parse_quote!('static));
            }
            InterfaceType::Sync => {
                p.push(syn::parse_quote!(Send));
                p.push(syn::parse_quote!(Sync));
                p.push(syn::parse_quote!('static));
            }
        }
        p
    }
}

pub(crate) fn transform_ast(items: &mut Vec<Item>, added_items: &mut Vec<Item>) {
    items.retain_mut(|item| {
        let mut keep = true;

        match item {
            Item::Impl(imp) => {
                for attr in &imp.attrs {
                    if attr.path().segments.len() == 2
                        && attr.path().segments[0].ident == "dylo"
                        && attr.path().segments[1].ident == "export"
                    {
                        let iface_typ = if let Ok(_meta) = attr.meta.require_path_only() {
                            Some(InterfaceType::Sync)
                        } else if let Ok(list) = attr.meta.require_list() {
                            if list.tokens.to_string().contains("nonsync") {
                                Some(InterfaceType::NonSync)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        if let Some(iface_typ) = iface_typ {
                            let tokens = (&imp).into_token_stream();
                            added_items.push(declare_trait(&tokens, &iface_typ)[0].clone());
                        }
                        keep = false
                    }
                }
            }
            Item::Enum(enm) => {
                filter_out_cfg_attr_impl(&mut enm.attrs);
            }
            Item::Struct(stru) => {
                filter_out_cfg_attr_impl(&mut stru.attrs);
                for f in stru.fields.iter_mut() {
                    filter_out_cfg_attr_impl(&mut f.attrs);
                }
            }
            _ => {
                // ignore
            }
        }

        if should_remove_item(item) {
            keep = false
        } else {
            // remove any cfg(not(feature = "impl")) attributes
            if let Some(attrs) = item_attributes(item) {
                attrs.retain(|attr| !is_cfg_not_feature_impl(attr));
            }
        }

        keep
    });
}

fn filter_out_cfg_attr_impl(attrs: &mut Vec<Attribute>) {
    attrs.retain_mut(|attr| {
        if let syn::Meta::List(list) = &attr.meta {
            if let Some(path_segment) = list.path.segments.first() {
                if path_segment.ident == "cfg_attr" {
                    if let Some(nested) = list.tokens.to_string().split(",").next() {
                        if nested.contains("feature") && nested.contains("\"impl\"") {
                            return false;
                        }
                    }
                }
            }
        }
        true
    })
}

fn declare_trait(tokens: &proc_macro2::TokenStream, iface_typ: &InterfaceType) -> Vec<Item> {
    let mut added_items = Vec::new();
    let file = syn::parse2::<syn::File>(tokens.clone()).unwrap();
    for item in &file.items {
        if let Item::Impl(imp) = item {
            if let Some((_, trait_path, _)) = &imp.trait_ {
                let mut trait_methods = Vec::new();

                for item in &imp.items {
                    if let ImplItem::Fn(fn_item) = item {
                        let trait_fn = syn::TraitItemFn {
                            attrs: fn_item.attrs.clone(),
                            sig: remove_mutable_bindings_from_sig(&fn_item.sig),
                            default: None,
                            semi_token: None,
                        };
                        trait_methods.push(trait_fn);
                    }
                }

                let trait_item = Item::Trait(syn::ItemTrait {
                    attrs: Vec::new(),
                    vis: syn::Visibility::Public(syn::token::Pub::default()),
                    unsafety: None,
                    auto_token: None,
                    restriction: None,
                    trait_token: syn::token::Trait::default(),
                    ident: trait_path.segments.last().unwrap().ident.clone(),
                    generics: imp.generics.clone(),
                    colon_token: None,
                    supertraits: iface_typ.supertraits(),
                    brace_token: syn::token::Brace::default(),
                    items: trait_methods.into_iter().map(syn::TraitItem::Fn).collect(),
                });

                added_items.push(trait_item);
            }
        }
    }
    added_items
}

fn remove_mutable_bindings_from_sig(sig: &syn::Signature) -> syn::Signature {
    let mut newsig = sig.clone();
    for input in &mut newsig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                if matches!(receiver.ty.as_ref(), Type::Reference(_)) {
                    // leave references alone, "&mut self" must be present in both
                    // the declaration and the implementation
                } else {
                    receiver.mutability = None;
                }
            }
            syn::FnArg::Typed(pat_type) => {
                if let syn::Pat::Ident(pat_ident) = &mut *pat_type.pat {
                    if matches!(pat_type.ty.as_ref(), Type::Reference(_)) {
                        // leave references alone, "&mut Vec<u8>" must be present in both
                        // the declaration and the implementation
                    } else {
                        pat_ident.mutability = None;
                    }
                }
            }
        }
    }

    newsig
}

fn get_latest_timestamp(path: &camino::Utf8Path) -> std::io::Result<SystemTime> {
    let mut latest = fs_err::metadata(path)?.modified()?;
    let mut latest_path = path.to_owned();

    if path.is_dir() {
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            let entry_path: &Utf8Path = entry.path().try_into().unwrap();
            if entry_path.components().any(|c| c.as_str() == ".con") {
                continue;
            }
            let timestamp = entry.metadata()?.modified()?;
            if timestamp > latest {
                latest = timestamp;
                latest_path = entry_path.to_owned();
            }
        }
    }

    tracing::debug!(
        "latest timestamp {} for {path} from {latest_path}",
        latest
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    Ok(latest)
}

fn setup_tracing_subscriber() {
    let filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            tracing_subscriber::filter::Targets::new().with_default(tracing::Level::INFO)
        });

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

struct Args {
    force: bool,
    mod_name: Option<String>,
}

fn parse_args() -> Args {
    let mut args = pico_args::Arguments::from_env();

    if args.contains(["-h", "--help"]) {
        println!("Generate consumer crates from mod implementations in a Cargo workspace");
        println!();
        println!("Usage: con [OPTIONS]");
        println!();
        println!("Options:");
        println!("  --force         Force regeneration of all consumer crates");
        println!("  --mod <NAME>    Only process the specified mod");
        println!("  -h, --help      Print help information");
        std::process::exit(0);
    }

    Args {
        force: args.contains("--force"),
        mod_name: args.opt_value_from_str("--mod").unwrap(),
    }
}

fn main() -> std::io::Result<()> {
    setup_tracing_subscriber();

    if !Utf8Path::new("Cargo.toml").exists() {
        tracing::error!("❌ Must be run from the root of a Cargo workspace");
        std::process::exit(1);
    }

    let args = parse_args();
    let mods_dir = Utf8Path::new(".");

    let mut mods = list_mods(mods_dir)?;
    tracing::info!("🔍 Found {} mods total", mods.len());

    if let Some(ref name) = args.mod_name {
        mods.retain(|m| m.name == *name);
        if mods.is_empty() {
            tracing::error!("❌ No mod found with name '{name}'");
            std::process::exit(1);
        }
        tracing::info!("🔍 Filtered to process mod '{name}'");
    }

    for mod_info in mods {
        process_mod(mod_info, args.force)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests;
