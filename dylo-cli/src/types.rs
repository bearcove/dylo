use camino::Utf8PathBuf;
use std::time::SystemTime;

pub const DYLO_RUNTIME_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Workspace,
    // if `mod-http`, then the name is `http`
    Module(String),
}

/// represents a mod crate we're managing, including both its impl & consumer versions.
/// contains paths and timestamps needed for monitoring file changes and determining when
/// regeneration of the consumer version is necessary
#[derive(Debug)]
pub struct ModInfo {
    /// human-readable name of the mod, extracted from the directory name (without mod- prefix)
    pub name: String,
    /// location of the mod's implementation code ($workspace/mod-$name/)
    pub mod_path: Utf8PathBuf,
    /// destination path for generating consumer version ($workspace/$name/)
    pub con_path: Utf8PathBuf,
    /// timestamp of most recently modified file in mod directory
    pub mod_timestamp: SystemTime,
    /// timestamp of most recently modified file in consumer directory
    pub con_timestamp: SystemTime,
}

/// Reason we might have to regenerate a mod's consumer version.
#[derive(Debug)]
pub enum ProcessReason {
    Force,
    Missing,
    Modified,
}

pub enum DyloCommand {
    Default {
        force: bool,
        scope: Scope,
    },
    Add {
        scope: Scope,
        is_impl: bool,
        deps: Vec<String>,
    },
    Rm {
        scope: Scope,
        deps: Vec<String>,
    },
}
