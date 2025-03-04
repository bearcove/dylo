use std::{collections::HashMap, time::SystemTime};

use camino::{Utf8Path, Utf8PathBuf};
use tracing::debug;

use crate::types::{ModInfo, Scope};

/// Lists all mods for a given scope
pub fn list_mods(workspace_root: &camino::Utf8Path, scope: Scope) -> eyre::Result<Vec<ModInfo>> {
    let mut mods = Vec::new();
    for entry in walkdir::WalkDir::new(workspace_root) {
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
        if let Scope::Module(ref module) = scope {
            if module != &name {
                continue;
            }
        }

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

pub fn get_single_mod(workspace_root: &camino::Utf8Path, scope: Scope) -> eyre::Result<ModInfo> {
    match scope {
        Scope::Workspace => {
            eyre::bail!(
                "This command expects a single module: use '--mod $NAME' to restrict the scope"
            );
        }
        Scope::Module(_) => {
            // all good
        }
    }
    let mods = list_mods(workspace_root, scope.clone())?;
    if mods.is_empty() {
        if let Scope::Module(name) = scope {
            tracing::error!("No module found with name: {name}");
        } else {
            tracing::error!("No modules found in workspace");
        }
        return Err(eyre::eyre!("No modules found"));
    }

    if mods.len() > 1 {
        tracing::error!("Found {} modules when expecting exactly one", mods.len());
        for m in &mods {
            tracing::debug!("Found module: {}", m.name);
        }
        return Err(eyre::eyre!(
            "Found multiple modules when expecting exactly one"
        ));
    }

    Ok(mods.into_iter().next().unwrap())
}

pub fn get_latest_timestamp(path: &camino::Utf8Path) -> std::io::Result<SystemTime> {
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

/// FileSet represents a set of files that need to be generated, stored in memory
/// before being written to disk. This allows checking if any files would actually
/// change before modifying them, which is important because Cargo uses file timestamps
/// to determine what needs to be rebuilt. By only writing files when their contents
/// would change, we avoid triggering unnecessary rebuilds just because timestamps
/// were updated.
///
/// See `-Z checksum-freshness`: <https://doc.rust-lang.org/nightly/cargo/reference/unstable.html#checksum-freshness>
#[derive(Debug, Clone)]
pub struct FileSet {
    pub files: HashMap<Utf8PathBuf, String>,
}

impl FileSet {
    pub fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    /// True if any files are missing from disk or have different contents.
    pub fn is_different(&self, root: &Utf8Path) -> std::io::Result<bool> {
        debug!(
            "Checking {count} files for differences in {root}",
            count = self.files.len(),
            root = root
        );
        let mut missing_count = 0;
        let mut changed_count = 0;

        for (rel_path, contents) in &self.files {
            let full_path = root.join(rel_path);
            if !full_path.exists() {
                debug!("File missing: {full_path}");
                missing_count += 1;
                continue;
            }

            let disk_contents = fs_err::read_to_string(&full_path)?;
            if &disk_contents != contents {
                debug!("File content different: {full_path}");
                changed_count += 1;
            }
        }

        let is_diff = missing_count > 0 || changed_count > 0;
        debug!(
            "Found {total} differences: {missing} missing, {changed} changed in {root}",
            total = missing_count + changed_count,
            missing = missing_count,
            changed = changed_count,
            root = root
        );

        Ok(is_diff)
    }

    /// Write file contents to disk, creating parent directories as needed.
    pub fn commit(&self, root: &Utf8Path) -> std::io::Result<()> {
        debug!(
            "📝 Committing {count} files to {root}",
            count = self.files.len(),
            root = root
        );
        for (rel_path, contents) in &self.files {
            let full_path = root.join(rel_path);
            if let Some(parent) = full_path.parent() {
                debug!("Creating directory {parent}");
                fs_err::create_dir_all(parent)?;
            }
            debug!("Writing file to {full_path} ({} bytes)", contents.len());
            fs_err::write(&full_path, contents)?;
        }
        Ok(())
    }
}

impl Default for FileSet {
    fn default() -> Self {
        Self::new()
    }
}
