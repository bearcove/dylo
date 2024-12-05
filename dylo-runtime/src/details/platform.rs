use std::{fmt::Display, sync::LazyLock};

#[derive(Debug)]
pub(crate) struct Extensions {
    pub lib: &'static str,
    pub dbg: &'static str,
}

impl Extensions {
    pub(crate) fn get() -> &'static Extensions {
        static EXTENSIONS: LazyLock<Extensions> = LazyLock::new(|| {
            if cfg!(target_os = "macos") {
                Extensions {
                    lib: "dylib",
                    dbg: "dSYM",
                }
            } else if cfg!(target_os = "linux") {
                Extensions {
                    lib: "so",
                    dbg: "so.dwp",
                }
            } else {
                panic!("Unsupported operating system - https://github.com/bearcove/dylo only supports MacOS and Linux for now");
            }
        });

        &EXTENSIONS
    }
}

pub(crate) static COLORS_ENABLED: LazyLock<bool> = LazyLock::new(|| {
    if let Ok(term_colors) = std::env::var("NO_COLOR") {
        return term_colors.is_empty();
    }

    if let Ok(force_color) = std::env::var("FORCE_COLOR") {
        return force_color != "0";
    }

    if let Ok(clicolor) = std::env::var("CLICOLOR") {
        return clicolor != "0";
    }

    if let Ok(clicolor_force) = std::env::var("CLICOLOR_FORCE") {
        return clicolor_force != "0";
    }

    // Default to enabled if no env vars are set
    true
});

pub(crate) fn colors_enabled() -> bool {
    *COLORS_ENABLED
}

struct ColorWrapper<T: Display> {
    color_code: u8,
    text: T,
    enabled: bool,
}

impl<T: Display> Display for ColorWrapper<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.enabled {
            write!(f, "\x1B[{}m{}\x1B[0m", self.color_code, self.text)
        } else {
            write!(f, "{}", self.text)
        }
    }
}

fn colorize(color_code: u8, t: impl Display) -> impl Display {
    ColorWrapper {
        color_code,
        text: t,
        enabled: colors_enabled(),
    }
}

pub(crate) fn blue(t: impl Display) -> impl Display {
    colorize(34, t)
}

pub(crate) fn red(t: impl Display) -> impl Display {
    colorize(31, t)
}

pub const RTLD_NOW: i32 = 0x2;

extern "C" {
    pub fn dlopen(filename: *const i8, flags: i32) -> *mut std::ffi::c_void;
    pub fn dlsym(handle: *mut std::ffi::c_void, symbol: *const i8) -> *mut std::ffi::c_void;
    pub fn dlerror() -> *mut i8;
}
