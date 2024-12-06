use std::collections::HashMap;
use std::ffi::CString;
use std::io::BufRead;
use std::process::{Command, Stdio};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use platform::{blue, dlerror, dlopen, dlsym, Extensions, RTLD_NOW};

// dummy trait just so we can make fat pointers
pub trait AnyMod: Send + Sync + 'static {}

// a loaded mod (type-erased `&'static dyn Mod`). note that
// this is a fat pointer, since it contains the address of
// the mod's vtable as well.
pub type AnyModRef = &'static dyn AnyMod;

mod platform;

#[derive(Debug)]
struct Paths {
    pub mod_srcdir: std::path::PathBuf,
    pub cargo_target_dir: std::path::PathBuf,
}

fn get_target_dir(mod_name: &str) -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("DYLO_TARGET_DIR") {
        return std::path::PathBuf::from(dir);
    }

    let home_dir = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .expect("Neither HOME nor USERPROFILE environment variables are set");

    std::path::PathBuf::from(home_dir)
        .join(".dylo-mods")
        .join(mod_name)
}

fn get_paths(mod_name: &str) -> Paths {
    let current_exe_path = std::env::current_exe().expect("Failed to get current executable path");
    let current_exe_folder = current_exe_path
        .parent()
        .expect("Failed to get parent directory of current executable");

    let base_dir = current_exe_folder
        .ancestors()
        .find(|p| p.join("Cargo.toml").exists())
        .unwrap_or(current_exe_folder);
    eprintln!("base dir: {:?}", base_dir);

    fn find_mod_dir(dir: &std::path::Path, mod_name: &str) -> Option<std::path::PathBuf> {
        if !dir.is_dir() {
            return None;
        }

        let dir_name = dir.file_name()?.to_str()?;
        if dir_name.starts_with(".")
            || dir_name.starts_with("target")
            || dir_name.starts_with("node_modules")
        {
            // no thanks
            return None;
        }

        if dir_name == format!("mod-{mod_name}") {
            return Some(dir.to_path_buf());
        }

        for entry in dir.read_dir().ok()? {
            let entry = entry.ok()?;
            let path = entry.path();

            if let Some(found) = find_mod_dir(&path, mod_name) {
                return Some(found);
            }
        }

        None
    }

    let mod_srcdir = find_mod_dir(base_dir, mod_name)
        .unwrap_or_else(|| panic!("Could not find mod source directory for mod {mod_name}"));
    let cargo_target_dir = get_target_dir(mod_name);

    Paths {
        mod_srcdir,
        cargo_target_dir,
    }
}

enum BuildMod {
    Dont,
    Yes,
    Verbosely,
}

impl BuildMod {
    fn from_env() -> Self {
        match std::env::var("DYLO_BUILD").as_deref() {
            Ok("0") => BuildMod::Dont,
            Ok("1") => BuildMod::Yes,
            Ok("verbose") => BuildMod::Verbosely,
            _ => BuildMod::Yes,
        }
    }

    fn should_build(&self) -> bool {
        match self {
            BuildMod::Dont => false,
            BuildMod::Yes => true,
            BuildMod::Verbosely => true,
        }
    }

    fn is_verbose(&self) -> bool {
        match self {
            BuildMod::Dont => false,
            BuildMod::Yes => false,
            BuildMod::Verbosely => true,
        }
    }
}

fn spawn_reader_thread(
    stream_name: String,
    stream: Option<Box<dyn std::io::Read + Send + 'static>>,
    tx: std::sync::mpsc::Sender<String>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        if let Some(stream) = stream {
            let reader = std::io::BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx.send(format!("[{}] {}", stream_name, line));
            }
        }
    })
}

fn build_mod(mod_name: &'static str) {
    let extensions = Extensions::get();
    let paths = get_paths(mod_name);
    let build_profile = if cfg!(debug_assertions) {
        "debug".to_string()
    } else {
        "release".to_string()
    };

    let before_build = Instant::now();
    let build_mode = BuildMod::from_env();

    if !build_mode.should_build() {
        return;
    }

    let current_exe_path = std::env::current_exe().expect("Failed to get current executable path");
    let current_exe_folder = current_exe_path.parent().unwrap();

    let dylib_path = format!(
        "{}/libmod_{}.{}",
        current_exe_folder.display(),
        mod_name,
        extensions.lib
    );
    let dylib_path_src = paths
        .cargo_target_dir
        .join(&build_profile)
        .join(format!("libmod_{}.{}", mod_name, extensions.lib));

    let debuginfo_path = format!(
        "{}/libmod_{}.{}",
        current_exe_folder.display(),
        mod_name,
        extensions.dbg
    );
    let debuginfo_path_src = paths
        .cargo_target_dir
        .join(&build_profile)
        .join(format!("libmod_{}.{}", mod_name, extensions.dbg));

    let noisy_builds = build_mode.is_verbose();

    let mut cmd = Command::new("cargo");
    cmd.env("CARGO_TARGET_DIR", &paths.cargo_target_dir);
    cmd.arg("build");
    cmd.arg("--verbose");
    cmd.arg("--features=impl,dylo-runtime/import-globals");
    if build_profile == "release" {
        cmd.arg("--release");
    }

    let mut child = if noisy_builds {
        cmd.current_dir(&paths.mod_srcdir)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to spawn cargo build process")
    } else {
        cmd.current_dir(&paths.mod_srcdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn cargo build process")
    };

    let (tx, rx) = std::sync::mpsc::channel();

    let stdout_thread = spawn_reader_thread(
        platform::blue("stdout").to_string(),
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        tx.clone(),
    );
    let stderr_thread = spawn_reader_thread(
        platform::red("stderr").to_string(),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        tx.clone(),
    );

    let collect_thread = std::thread::spawn(move || {
        let mut output = Vec::new();
        for line in rx {
            output.push(line);
        }
        output
    });

    drop(tx);

    let status = child
        .wait()
        .expect("Failed to wait for cargo build process");

    stdout_thread.join().unwrap();
    stderr_thread.join().unwrap();
    let lines = collect_thread.join().unwrap();

    if !status.success() {
        eprintln!("\nFailed to build {mod_name}, build log follows:");
        eprintln!("\n==========");
        for line in lines {
            eprintln!("{line}");
        }
        eprintln!("==========\n");

        panic!("Failed to build {mod_name}");
    }

    std::fs::copy(&dylib_path_src, &dylib_path).unwrap_or_else(|_| {
        panic!(
            "Failed to copy built module from {} to {}",
            dylib_path_src.display(),
            dylib_path
        )
    });

    if std::path::Path::new(&debuginfo_path_src).exists() {
        std::fs::copy(&debuginfo_path_src, &debuginfo_path).unwrap_or_else(|_| {
            panic!(
                "Failed to copy debug info from {} to {}",
                debuginfo_path_src.display(),
                debuginfo_path
            )
        });
    }

    eprintln!(
        "📦 Built {} in {:?}",
        blue(mod_name),
        before_build.elapsed()
    );
}

type LockSlot = Arc<Mutex<Option<AnyModRef>>>;

// keep locks per build directory, exported by rubicon. this avoids
// rebuilding the same mod over and over — cargo can take up to ~200ms
// on macOS to go through a no-op build, so, this matters.
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

    // if not, keep going (and hold the lock slot the whole time)
    build_mod(mod_name);

    let extensions = Extensions::get();
    let current_exe = std::env::current_exe().expect("Failed to get current executable path");
    let current_exe_folder = current_exe.parent().unwrap();

    let dylib_path = current_exe_folder
        .join(format!("libmod_{}.{}", mod_name, extensions.lib))
        .into_os_string()
        .into_string()
        .unwrap();

    let before_load = Instant::now();

    let dylib_path = CString::new(dylib_path).expect("Invalid path");
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

    eprintln!(
        "📦 Loaded \x1B[34m{mod_name}\x1B[0m in {:?}",
        before_load.elapsed()
    );

    *locked_slot = Some(plugin);
    plugin
}
