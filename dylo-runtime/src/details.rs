use std::collections::HashMap;
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use platform::{Extensions, RTLD_NOW, blue, dlerror, dlopen, dlsym};

// dummy trait just so we can make fat pointers
pub trait AnyMod: Send + Sync + 'static {}

// a loaded mod (type-erased `&'static dyn Mod`). note that
// this is a fat pointer, since it contains the address of
// the mod's vtable as well.
pub type AnyModRef = &'static dyn AnyMod;

static DYLO_DEBUG: LazyLock<bool> =
    LazyLock::new(|| matches!(std::env::var("DYLO_DEBUG").as_deref(), Ok("1")));

macro_rules! debug {
    ($($arg:tt)*) => {
        if *DYLO_DEBUG {
            eprintln!($($arg)*);
        }
    };
}

mod platform;

struct SearchPaths {
    paths: Vec<PathBuf>,
}

impl SearchPaths {
    fn from_env() -> Self {
        let mut paths = Vec::new();

        debug!("dylo search paths:");
        if let Ok(dir) = std::env::var("DYLO_MOD_DIR") {
            let path = PathBuf::from(dir);
            if !path.is_absolute() {
                panic!(
                    "$DYLO_MOD_DIR must be an absolute path, refusing to proceed. (DYLO_MOD_DIR was set to {})",
                    blue(path.display())
                );
            }
            if !path.exists() {
                panic!(
                    "$DYLO_MOD_DIR must exist. (DYLO_MOD_DIR was set to {})",
                    blue(path.display())
                );
            }
            paths.push(path);
        } else {
            debug!("(note: you can set $DYLO_MOD_DIR to prepend your own search path)");
        }

        let exe_path = std::env::current_exe().unwrap_or_else(|e| {
            debug!("Unable to get current executable path: {e}");
            PathBuf::new()
        });
        debug!("Current executable path: {}", blue(exe_path.display()));

        let real_exe_path = match exe_path.canonicalize() {
            Ok(canonical_path) => canonical_path,
            Err(e) => {
                debug!("Failed to canonicalize executable path: {e}");
                exe_path.clone()
            }
        };
        debug!("Real executable path: {}", blue(real_exe_path.display()));

        if let Some(exe_dir) = real_exe_path.parent() {
            debug!("Executable directory: {}", blue(exe_dir.display()));
            let potential_paths = vec![
                exe_dir.join("../libexec"),
                exe_dir.join("../../libexec/release"),
                exe_dir.to_path_buf(),
            ];

            for path in potential_paths {
                match path.canonicalize() {
                    Ok(canonical_path) => {
                        debug!("Canonical path: {}", blue(canonical_path.display()));
                        paths.push(canonical_path);
                    }
                    Err(e) => {
                        debug!("Error canonicalizing path {}: {}", blue(path.display()), e);
                        paths.push(path);
                    }
                }
            }
        } else {
            debug!(
                "Unable to get parent directory of executable: {}",
                blue(real_exe_path.display())
            );
        }

        for path in &paths {
            debug!("  {}", path.display());
        }

        Self { paths }
    }

    fn find_module(&self, mod_name: &str) -> Option<PathBuf> {
        let extensions = Extensions::get();
        let file_name = format!("libmod_{}.{}", mod_name, extensions.lib);

        for path in &self.paths {
            let full_path = path.join(&file_name);
            debug!("Looking for module in: {}", blue(full_path.display()));
            if full_path.exists() {
                debug!("Found module at: {}", blue(full_path.display()));
                return Some(full_path);
            }
        }

        debug!("Module not found: {}", blue(mod_name));
        None
    }
}

type LockSlot = Arc<Mutex<Option<AnyModRef>>>;

// keep locks per module name, exported by rubicon.
rubicon::process_local! {
    static LOCKS: LazyLock<Mutex<HashMap<String, LockSlot>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
}

pub fn load_mod(mod_name: &'static str) -> AnyModRef {
    let slot = {
        let mut locks = LOCKS.lock().unwrap();
        locks.entry(mod_name.to_string()).or_default().clone()
    };
    let mut locked_slot = slot.lock().unwrap();
    if let Some(fat_pointer) = locked_slot.as_ref() {
        // if we've already loaded the mod, return the same address
        return *fat_pointer;
    }

    let search_paths = SearchPaths::from_env();
    let dylib_path = search_paths
        .find_module(mod_name)
        .unwrap_or_else(|| panic!("dylo could not find find module: {}", mod_name));

    let before_load = Instant::now();

    let dylib_path = CString::new(dylib_path.to_str().unwrap()).expect("Invalid path");
    let handle = unsafe { dlopen(dylib_path.as_ptr(), RTLD_NOW) };
    if handle.is_null() {
        let err = unsafe { std::ffi::CStr::from_ptr(dlerror()) }
            .to_string_lossy()
            .into_owned();
        panic!("Failed to load dynamic library: {}", err);
    }

    // note: we never dlclose the handle, on purpose.

    let symbol_name = CString::new("github.com_bearcove_dylo").unwrap();
    let init_sym = unsafe { dlsym(handle, symbol_name.as_ptr()) };
    if init_sym.is_null() {
        let err = unsafe { std::ffi::CStr::from_ptr(dlerror()) }
            .to_string_lossy()
            .into_owned();
        panic!("Did not find in dynamic library: {}", err);
    }

    type InitFn = unsafe extern "Rust" fn() -> AnyModRef;
    let init_fn: InitFn = unsafe { std::mem::transmute(init_sym) };
    let plugin = unsafe { init_fn() };

    debug!(
        "📦 Loaded {} in {:?}",
        blue(mod_name),
        before_load.elapsed()
    );

    *locked_slot = Some(plugin);
    plugin
}
