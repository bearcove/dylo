/// This is the entry point for this module when loaded dynamically.
///
/// See <https://github.com/bearcove/dylo>
#[doc(hidden)]
#[export_name = "https://github.com/bearcove/dylo"]
pub extern "Rust" fn awaken() -> &'static (dyn crate::Mod) {
    let m: crate::ModImpl = std::default::Default::default();
    let m: std::boxed::Box<dyn crate::Mod> = std::boxed::Box::new(m);
    std::boxed::Box::leak(m)
}
