#[cfg(feature = "impl")]
use impl_only;

#[cfg(feature = "impl")]
fn impl_only() {}

#[cfg(not(feature = "impl"))]
fn not_impl_only() {}

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

#[dylo::export]
impl Mod for ModImpl {
    fn foo(&self) -> u32 {
        42
    }
}

#[cfg(feature = "impl")]
enum ImplEnum {
    Variant1,
    Variant2,
}

#[cfg(feature = "impl")]
impl ImplEnum {
    fn variant1(&self) -> u32 {
        23
    }
}

enum NotImplEnum {
    Variant1,
    Variant2,
}

impl NotImplEnum {
    fn variant1(&self) -> u32 {
        23
    }
}
