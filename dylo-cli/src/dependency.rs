use camino::{Utf8Path, Utf8PathBuf};
use std::process::Command as ProcessCommand;
use toml_edit::{Array, DocumentMut, Item};

/// Add a dependency to a mod's Cargo.toml
///
/// If is_impl is true, the dependency will be marked as optional and enabled by the impl feature
pub fn add_dependency(mod_path: &Utf8Path, deps: &[String], is_impl: bool) -> std::io::Result<()> {
    // First, use cargo add to add the dependency
    let mut command = ProcessCommand::new("cargo");

    // Set current directory to the workspace root (parent of the mod directory)
    // This is needed so cargo can find the package properly
    if let Some(workspace_root) = mod_path.parent() {
        command.current_dir(workspace_root);
    }

    command
        .arg("add")
        .arg("--package")
        .arg(mod_path.file_name().unwrap())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    if is_impl {
        command.arg("--optional");
    }

    for dep in deps {
        command.arg(dep);
    }

    tracing::debug!("Running: {:?}", command);
    let status = command.status()?;

    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to add dependency: cargo add exited with status {status}"),
        ));
    }

    // If this is an impl-only dependency, update the impl feature and remove auto-generated feature
    if is_impl {
        let cargo_toml_path = mod_path.join("Cargo.toml");
        let cargo_toml_content = fs_err::read_to_string(&cargo_toml_path)?;
        let mut doc = cargo_toml_content
            .parse::<DocumentMut>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Add to impl feature
        ensure_impl_feature(&mut doc, deps)?;

        // Remove auto-generated features created by cargo add
        if let Some(features_table) = doc.get_mut("features").and_then(|f| f.as_table_mut()) {
            for dep in deps {
                let dep_name = match dep.split_once('@') {
                    Some((name, _)) => name,
                    None => dep,
                };

                // Remove the feature with the same name as the dependency
                features_table.remove(dep_name);
            }
        }

        // Write the updated Cargo.toml back to disk
        fs_err::write(&cargo_toml_path, doc.to_string())?;
    }

    Ok(())
}

/// Helper function to ensure impl feature exists and includes the given dependencies
fn ensure_impl_feature(doc: &mut DocumentMut, deps: &[String]) -> std::io::Result<()> {
    // Ensure we have a features table
    if !doc.contains_key("features") {
        doc["features"] = Item::Table(Default::default());
    }

    // Ensure the impl feature exists
    let features = doc["features"].as_table_mut().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "features is not a table")
    })?;

    if !features.contains_key("impl") {
        features.insert("impl", toml_edit::value(Array::new()));
    }

    // Add the dependency to the impl feature if it doesn't already exist
    let impl_feature = features["impl"].as_array_mut().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "impl is not an array")
    })?;

    for dep in deps {
        let dep_name = match dep.split_once('@') {
            Some((name, _)) => name,
            None => dep,
        };

        let dep_feature = format!("dep:{dep_name}");
        if !impl_feature
            .iter()
            .any(|v| v.as_str() == Some(&dep_feature))
        {
            impl_feature.push(dep_feature);
        }
    }

    Ok(())
}

/// Remove a dependency from a mod's Cargo.toml
///
/// Also removes it from the impl feature if it was enabled there
pub fn remove_dependency(mod_path: &Utf8Path, deps: &[String]) -> std::io::Result<()> {
    // Use cargo rm to remove the dependency
    let mut command = ProcessCommand::new("cargo");

    // Set current directory to the workspace root (parent of the mod directory)
    // This is needed so cargo can find the package properly
    if let Some(workspace_root) = mod_path.parent() {
        command.current_dir(workspace_root);
    }

    command
        .arg("rm")
        .arg("--package")
        .arg(mod_path.file_name().unwrap())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    for dep in deps {
        command.arg(dep);
    }

    tracing::debug!("Running: {:?}", command);
    let status = command.status()?;

    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to remove dependency: cargo rm exited with status {status}"),
        ));
    }

    // Also need to remove from impl feature if it exists
    let cargo_toml_path = mod_path.join("Cargo.toml");
    let cargo_toml_content = fs_err::read_to_string(&cargo_toml_path)?;
    let mut doc = cargo_toml_content
        .parse::<DocumentMut>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    // Check if we have a features table with an impl feature
    if let Some(features) = doc.get_mut("features").and_then(|f| f.as_table_mut()) {
        if let Some(impl_feature) = features.get_mut("impl").and_then(|i| i.as_array_mut()) {
            // Filter out the removed dependencies
            let original_len = impl_feature.len();

            // Create a new array without the deleted deps
            let mut new_array = Array::new();
            for item in impl_feature.iter() {
                if let Some(feature_str) = item.as_str() {
                    let mut should_keep = true;

                    for dep in deps {
                        let dep_feature = format!("dep:{dep}");
                        if feature_str == dep_feature {
                            should_keep = false;
                            break;
                        }
                    }

                    if should_keep {
                        new_array.push(feature_str);
                    }
                }
            }

            // Replace with the filtered array
            if new_array.len() != original_len {
                *impl_feature = new_array;

                // Write the updated Cargo.toml back to disk
                fs_err::write(&cargo_toml_path, doc.to_string())?;
            }
        }
    }

    Ok(())
}

/// Find mod directory by name
pub fn find_mod_by_name(workspace_root: &Utf8Path, mod_name: &str) -> Option<Utf8PathBuf> {
    let mod_dir_name = format!("mod-{mod_name}");

    // First check if there's a direct mod-foo directory in the workspace
    let direct_path = workspace_root.join(&mod_dir_name);
    if direct_path.exists() && direct_path.is_dir() {
        return Some(direct_path);
    }

    // Then try to find it under crates/
    let crates_path = workspace_root.join("crates").join(&mod_dir_name);
    if crates_path.exists() && crates_path.is_dir() {
        return Some(crates_path);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    // Note: These tests run real `cargo` commands in isolated temporary directories.
    // The cargo binary must be available in the PATH for these tests to work.
    // All operations are contained in the temporary directories, so they won't
    // affect the actual workspace or system.
    //
    // The test creates a temporary directory with this structure:
    //
    // temp_dir/
    // ├── Cargo.toml (workspace root)
    // └── crates/
    //     └── mod-test/
    //         ├── Cargo.toml
    //         └── src/
    //             └── lib.rs

    /// Helper function to create a temporary workspace with a mod-test crate
    /// Returns (temp_dir, mod_path) where:
    ///   - temp_dir is the tempdir handle (must be kept alive)
    ///   - mod_path is the path to the mod-test crate
    fn setup_test_workspace() -> (tempfile::TempDir, Utf8PathBuf) {
        // Create a temporary directory with test Cargo workspace
        let dir = tempdir().unwrap();
        let dir_path = Utf8PathBuf::try_from(dir.path().to_path_buf()).unwrap();

        // Create workspace Cargo.toml with resolver 3 for edition 2024
        let workspace_cargo = r#"[workspace]
members = ["crates/*"]
resolver = "3"
"#;
        fs_err::write(dir_path.join("Cargo.toml"), workspace_cargo).unwrap();

        // Create the crates directory
        fs_err::create_dir(dir_path.join("crates")).unwrap();

        // Create the mod-test directory and its Cargo.toml
        let mod_path = dir_path.join("crates").join("mod-test");
        fs_err::create_dir(&mod_path).unwrap();

        let cargo_content = fs_err::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("src")
                .join("testdata")
                .join("test-cargo.toml"),
        )
        .unwrap();

        fs_err::write(mod_path.join("Cargo.toml"), cargo_content).unwrap();

        // Create src directory and lib.rs to make it a valid crate
        fs_err::create_dir(mod_path.join("src")).unwrap();
        fs_err::write(mod_path.join("src/lib.rs"), "// Empty lib file\n").unwrap();

        (dir, mod_path)
    }

    #[test]
    fn test_add_dependency() {
        // Setup the test workspace
        let (dir, mod_path) = setup_test_workspace();

        // Add a regular dependency
        add_dependency(&mod_path, &["serde".to_string()], false).unwrap();

        // Add an impl-only dependency
        add_dependency(&mod_path, &["tokio".to_string()], true).unwrap();

        // Read the final Cargo.toml content and create a snapshot
        let final_content = fs_err::read_to_string(mod_path.join("Cargo.toml")).unwrap();
        insta::assert_snapshot!("cargo_toml_after_add", final_content);

        drop(dir); // Ensure temp directory is cleaned up
    }

    #[test]
    fn test_remove_dependency() {
        // Setup the test workspace
        let (dir, mod_path) = setup_test_workspace();

        // First add both dependencies
        add_dependency(&mod_path, &["serde".to_string()], false).unwrap();
        add_dependency(&mod_path, &["tokio".to_string()], true).unwrap();

        // Then remove the impl-only dependency
        remove_dependency(&mod_path, &["tokio".to_string()]).unwrap();

        // Read the final Cargo.toml content and create a snapshot
        let final_content = fs_err::read_to_string(mod_path.join("Cargo.toml")).unwrap();
        insta::assert_snapshot!("cargo_toml_after_remove", final_content);

        drop(dir); // Ensure temp directory is cleaned up
    }
}
