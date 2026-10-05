#![cfg(all(feature = "inventory", feature = "persistence"))]

#[rustversion::all(stable, since(1.90))]
#[test]
fn compile_pass() {
    let t = trybuild::TestCases::new();
    t.pass("tests/compile-pass/*.rs");
}
