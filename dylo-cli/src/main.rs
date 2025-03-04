use camino::Utf8PathBuf;
use command::{parse_args, run_command};
use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _};
use types::Scope;

// note: init_template and load_template are NOT modules here

pub mod codegen;
pub mod command;
pub mod dependency;
pub mod types;
pub mod workspace;

const SPEC_PATH: &str = ".dylo/spec.rs";
const SUPPORT_PATH: &str = ".dylo/support.rs";

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

fn main() -> eyre::Result<()> {
    setup_tracing_subscriber();

    // Find workspace root by looking for Cargo.toml or .git directory
    let mut current_dir =
        camino::Utf8PathBuf::from_path_buf(std::env::current_dir().unwrap()).unwrap();

    let mut ambient_scope = Scope::Workspace;
    let workspace_root = loop {
        let cargo_toml_path = current_dir.join("Cargo.toml");
        let git_dir_path = current_dir.join(".git");

        if cargo_toml_path.exists() {
            // Found a Cargo.toml file, check if it's a workspace
            let cargo_content = fs_err::read_to_string(&cargo_toml_path)?;
            let doc = cargo_content.parse::<toml_edit::DocumentMut>().unwrap();

            if doc.contains_key("workspace") {
                // It's a workspace, use this as the root
                tracing::debug!("Found workspace root at {current_dir}");
                break current_dir;
            } else if let Some(package) = doc.get("package").and_then(|p| p.as_table()) {
                // It's a package, check if it's a mod
                if let Some(name) = package.get("name").and_then(|n| n.as_str()) {
                    if name.starts_with("mod-") {
                        let mod_name = name.trim_start_matches("mod-").to_string();
                        ambient_scope = Scope::Module(mod_name);
                    }
                }
                tracing::debug!("Found package at {current_dir}");
                // Continue searching for the workspace root
            }
        }

        if git_dir_path.exists() && git_dir_path.is_dir() {
            // Found a .git directory without finding a workspace first, so error out
            tracing::error!(
                "Reached a Git repository at {current_dir} without finding a Cargo workspace"
            );
            std::process::exit(1);
        }

        // Try going up one directory
        if !current_dir.pop() {
            // Reached the filesystem root without finding anything
            tracing::warn!("Could not find workspace root, using current directory");
            break Utf8PathBuf::from(".");
        }
    };

    let command = parse_args(ambient_scope);
    run_command(workspace_root, command)?;

    Ok(())
}

#[cfg(test)]
mod tests;
