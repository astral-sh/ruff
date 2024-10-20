/// Rebuild the crate if a test file is added or removed from
pub fn main() {
    println!("cargo::rerun-if-changed=resources/mdtest");
}
