/// Loads the module (building it if necessary) and returns a 'static reference to it.
///
/// Note that modules are not meant to be unloaded.
///
/// See <https://github.com/bearcove/dylo>
pub fn load() -> &'static (dyn Mod) {
    static MOD: ::std::sync::LazyLock<&'static (dyn Mod)> = ::std::sync::LazyLock::new(|| {
        let fat_pointer = ::dylo_runtime::details::load_mod(env!("CARGO_PKG_NAME"));
        unsafe {
            ::std::mem::transmute::<::dylo_runtime::details::AnyModRef, &'static dyn Mod>(
                fat_pointer,
            )
        }
    });
    *MOD
}
