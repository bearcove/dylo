/*
 * Here is the plan: `mods/` contains "mods", Rust crates that have an "impl" feature and a "consumer" feature.
 *
 * Their crate name is `mod-<name>` — we want to generate crates named `con-<name>`, that have everything
 * except anything under a `#[cfg(feature = "impl")]` section (which includes `struct ModImpl`, `impl Mod for ModImpl`, etc.)
 *
 * The key change is looking for `#[con::export]` attributes on impl blocks. When we find these:
 * 1. Generate corresponding trait definitions
 * 2. Write them to src/.con/spec.rs with a machine-generated notice
 * 3. Make sure there is an include statement for this file in the mod's lib.rs
 * 4. Copy the generated spec.rs file to the con version as-is
 *
 * The generated spec.rs file should:
 * - Have a clear machine-generated notice and regeneration instructions
 * - Not be counted in mod timestamps since it's generated
 * - Contain all trait definitions derived from #[con::export] impls
 *
 * The 'con' command-line utility will:
 *  1. List all mods in `mods/`
 *  2. For each mod:
 *    2a. Check timestamps of all files under mod directory (excluding .con/spec.rs)
 *    2b. Check for existence of con directory and its most recent timestamp
 *    2c. If force flag is passed, or con directory is missing, or mod timestamps are newer:
 *      2c1. Parse mod's lib.rs with syn and:
 *        - Strip #[cfg(feature = "impl")] items
 *        - Find #[con::export] impls and generate traits
 *        - Write traits to src/.con/spec.rs
 *      2c2. Generate new `Cargo.toml` based on mod's `Cargo.toml`, updating name
 *      2c3. Generate full tree for con version including copied spec.rs
 *      2c4. Compare with existing con version (if it exists)
 *      2c5. If they differ (or didn't exist):
 *        2c5a. Write generated Cargo.toml to con directory
 *        2c5b. Write generated lib.rs and spec.rs to con directory
 *        2c5c. Run `cargo check -p con-${mod_name}` to verify compilation
 */

use std::collections::HashMap;

use camino::{Utf8Path, Utf8PathBuf};
use proc_macro2 as _;
use quote::ToTokens;
use syn::{Attribute, ImplItem, Item, Type};
use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};

#[derive(Debug)]
struct ModInfo {
    name: String,
    mod_path: camino::Utf8PathBuf,
    con_path: camino::Utf8PathBuf,
    mod_timestamp: std::time::SystemTime,
    con_timestamp: std::time::SystemTime,
}

#[derive(Debug)]
enum ProcessReason {
    Force,
    Missing,
    Modified,
}

fn list_mods(mods_dir: &camino::Utf8Path) -> std::io::Result<Vec<ModInfo>> {
    let mut mods = Vec::new();
    for entry in fs_err::read_dir(mods_dir)? {
        let entry = entry?;
        let mod_path: camino::Utf8PathBuf = entry.path().try_into().unwrap();

        if !mod_path.is_dir() {
            continue;
        }

        let name = mod_path.file_name().unwrap().to_string();
        if !name.starts_with("mod-") {
            continue;
        }

        let name = name.trim_start_matches("mod-").to_string();
        let con_path = mods_dir.join(format!("con-{name}"));

        // Check timestamps
        let mod_timestamp = get_latest_timestamp(&mod_path)?;
        let con_timestamp = if con_path.exists() {
            get_latest_timestamp(&con_path)?
        } else {
            std::time::SystemTime::UNIX_EPOCH
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

fn mod_cargo_to_con_cargo(mod_info: &ModInfo) -> std::io::Result<String> {
    // Parse the TOML doc into an editable format
    let mod_cargo = fs_err::read_to_string(mod_info.mod_path.join("Cargo.toml"))?;
    let mut doc = mod_cargo.parse::<toml_edit::DocumentMut>().unwrap();

    // Update package name to be prefixed with "con-"
    doc["package"]["name"] = toml_edit::value(format!("con-{}", mod_info.name));

    // If there's a features table, update the default features
    // to only include "consumer" since this is the consumer crate
    if let Some(features) = doc.get_mut("features") {
        if let Some(default) = features.get_mut("default") {
            *default = toml_edit::value(toml_edit::Array::from_iter(["consumer"]));
        }
    }

    // Remove dev-dependencies section if it exists
    if doc.contains_key("dev-dependencies") {
        doc.remove("dev-dependencies");
    }

    // Remove con dependency if it exists
    if let Some(deps) = doc.get_mut("dependencies") {
        if deps.is_table() {
            deps.as_table_mut().unwrap().remove("con");
        }
    }

    Ok(doc.to_string())
}

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

    tracing::info!("📦 Processing mod {} ({:?})", mod_info.name, reason);

    // Generate consumer version by parsing and filtering lib.rs
    tracing::info!("⚙️ Parsing mod {}", mod_info.name);
    let start = std::time::Instant::now();

    let lib_rs = fs_err::read_to_string(mod_info.mod_path.join("src/lib.rs"))?;
    let ast = syn::parse_file(&lib_rs).unwrap();

    // Check for include statement for .con/spec.rs
    let includes_spec = ast.items.iter().any(|item| {
        if let Item::Macro(mac) = item {
            if mac.mac.path.is_ident("include") {
                let tokens = mac.mac.tokens.to_string();
                return tokens.contains(".con/spec.rs");
            }
        }
        false
    });

    tracing::debug!("include .con/spec.rs statement present: {includes_spec}");

    let mut con_items: Vec<Item> = ast.items.clone();
    let mut spec_items: Vec<Item> = Default::default();
    transform_macro_items(&mut con_items, &mut spec_items);

    let duration = start.elapsed();

    let spec_ast = syn::File {
        shebang: None,
        attrs: vec![
            syn::parse_quote! {
                #[doc = "// This file was automatically generated by the `conman` utility."]
            },
            syn::parse_quote! {
                #[doc = "// To regenerate this file, run `conman` in the root directory."]
            },
            syn::parse_quote! {
                #[doc = "// Do not edit this file directly - your changes will be overwritten."]
            },
        ],
        items: spec_items,
    };

    let spec_expanded = spec_ast.into_token_stream().to_string();
    let spec_formatted = rustfmt_wrapper::rustfmt(spec_expanded).unwrap();

    let con_ast = syn::File {
        shebang: None,
        attrs: vec![
            syn::parse_quote! {
                #[doc = "// This file was automatically generated by the `con` utility: https://github.com/bearcove/con"]
            },
            syn::parse_quote! {
                #[doc = "// To regenerate this file, run `con` in the root directory."]
            },
            syn::parse_quote! {
                #[doc = "// Do not edit this file directly - your changes will be overwritten."]
            },
        ],
        items: con_items,
    };

    let con_expanded = con_ast.into_token_stream().to_string();
    let con_formatted = rustfmt_wrapper::rustfmt(con_expanded).unwrap();

    tracing::info!(
        "📝 Parsed {} in {:.2}s, size: {} bytes",
        mod_info.name,
        duration.as_secs_f32(),
        lib_rs.len()
    );

    // Generate files for mod version
    let mut mod_files = FileSet::new();
    // Add spec.rs to mod version
    mod_files
        .files
        .insert("src/.con/spec.rs".into(), spec_formatted.clone());

    if !includes_spec {
        let content = format!("// Include autogenerated interface specifications\ninclude!(\".con/spec.rs\");\n\n{lib_rs}");
        mod_files.files.insert("src/lib.rs".into(), content);
    }

    // Generate files for consumer version
    let mut con_files = FileSet::new();
    // Generate Cargo.toml
    let con_cargo = mod_cargo_to_con_cargo(&mod_info)?;
    con_files.files.insert("Cargo.toml".into(), con_cargo);
    // Add lib.rs and spec.rs
    con_files.files.insert("src/lib.rs".into(), con_formatted);
    con_files
        .files
        .insert("src/.con/spec.rs".into(), spec_formatted);

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
            .args([
                "check",
                "--package",
                &format!("con-{}", mod_info.name),
                "--no-default-features",
                "--features",
                "consumer",
            ])
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
        }
    }
    Ok(())
}

fn item_attributes(item: &Item) -> Option<&Vec<Attribute>> {
    match item {
        Item::Const(item) => Some(&item.attrs),
        Item::Enum(item) => Some(&item.attrs),
        Item::ExternCrate(item) => Some(&item.attrs),
        Item::Fn(item) => Some(&item.attrs),
        Item::ForeignMod(item) => Some(&item.attrs),
        Item::Impl(item) => Some(&item.attrs),
        Item::Macro(item) => Some(&item.attrs),
        Item::Mod(item) => Some(&item.attrs),
        Item::Static(item) => Some(&item.attrs),
        Item::Struct(item) => Some(&item.attrs),
        Item::Trait(item) => Some(&item.attrs),
        Item::TraitAlias(item) => Some(&item.attrs),
        Item::Type(item) => Some(&item.attrs),
        Item::Union(item) => Some(&item.attrs),
        Item::Use(item) => Some(&item.attrs),
        Item::Verbatim(_) => None,
        _ => None,
    }
}

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

fn should_remove_item(item: &Item) -> bool {
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

fn transform_macro_items(items: &mut Vec<Item>, added_items: &mut Vec<Item>) {
    items.retain(|item| {
        let mut keep = true;
        if let Item::Impl(imp) = item {
            for attr in &imp.attrs {
                if attr.path().segments.len() == 2
                    && attr.path().segments[0].ident == "con"
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
        if should_remove_item(item) {
            keep = false
        }
        keep
    });
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

fn get_latest_timestamp(path: &camino::Utf8Path) -> std::io::Result<std::time::SystemTime> {
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
    let mods_dir = Utf8Path::new("mods");

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
