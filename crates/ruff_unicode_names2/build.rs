use std::{env, path::PathBuf};
use unicode_names2_generator as generator;

/// [UnicodeData.txt] contains Unicode Character Data
///
/// [UnicodeData.txt]: https://www.unicode.org/Public/17.0.0/ucd/UnicodeData.txt
const UNICODE_DATA: &str = include_str!("data/UnicodeData.txt");
/// Unicode aliases
///
/// [NamesList.txt] contents contains a map of unicode aliases to their corresponding values.
///
/// [NamesList.txt]: https://www.unicode.org/Public/17.0.0/ucd/NameAliases.txt
const NAME_ALIASES: &str = include_str!("data/NameAliases.txt");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=data/");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    {
        let mut generated_path = out_dir.clone();
        generated_path.push("generated.rs");
        generator::generate(UNICODE_DATA, Some(&generated_path), None);
    }
    {
        let mut generated_phf_path = out_dir.clone();
        generated_phf_path.push("generated_phf.rs");
        generator::generate_phf(UNICODE_DATA, Some(&generated_phf_path), None, 3, 2);
    }
    {
        let mut generated_alias_path = out_dir;
        generated_alias_path.push("generated_alias.rs");
        generator::generate_aliases(NAME_ALIASES, &generated_alias_path);
    }
}
