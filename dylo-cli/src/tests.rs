use codegen::transform_ast;

use super::*;

#[test]
fn snapshot_simple_module() {
    let input_rs = include_str!("testdata/simple-module.rs");
    let mut file = syn::parse_file(input_rs).unwrap();

    let mut added_items = Vec::new();
    transform_ast(&mut file.items, &mut added_items);

    file.items.extend(added_items);

    let output = prettyplease::unparse(&file);
    insta::assert_snapshot!(output);
}
