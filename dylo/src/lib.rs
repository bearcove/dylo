use unsynn::*;

unsynn! {
    keyword Impl = "impl";
    keyword For = "for";

    struct ImplTraitForStruct {
        _impl: Impl,
        trait_name: Ident,
        _for: For,
        struct_name: Ident,
        body: BraceGroupContaining<TokenStream>,
    }
}

#[proc_macro_attribute]
pub fn export(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let item = TokenStream::from(item);
    let mut token_iter = item.to_token_iter();
    let ast = ImplTraitForStruct::parse(&mut token_iter).unwrap();
    panic!("{:?}", ast);
}
