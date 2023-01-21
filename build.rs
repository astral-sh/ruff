use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    generate_linter_name_and_url(&out_dir);
}

const RULES_SUBMODULE_DOC_PREFIX: &str = "//! Rules from ";

/// The `src/rules/*/mod.rs` files are expected to have a first line such as the
/// following:
///
///     //! Rules from [Pyflakes](https://pypi.org/project/pyflakes/2.5.0/).
///
/// This function extracts the link label and url from these comments and
/// generates the `name` and `url` functions for the `Linter` enum
/// accordingly, so that they can be used by `ruff_dev::generate_rules_table`.
fn generate_linter_name_and_url(out_dir: &Path) {
    println!("cargo:rerun-if-changed=src/rules/");

    let mut name_match_arms: String = r#"Linter::Ruff => "Ruff-specific rules","#.into();
    let mut url_match_arms: String = r#"Linter::Ruff => None,"#.into();

    for file in fs::read_dir("src/rules/")
        .unwrap()
        .flatten()
        .filter(|f| f.file_type().unwrap().is_dir() && f.file_name() != "ruff")
    {
        let mod_rs_path = file.path().join("mod.rs");
        let mod_rs_path = mod_rs_path.to_str().unwrap();
        let first_line = BufReader::new(fs::File::open(mod_rs_path).unwrap())
            .lines()
            .next()
            .unwrap()
            .unwrap();

        let Some(comment) = first_line.strip_prefix(RULES_SUBMODULE_DOC_PREFIX) else {
                panic!("expected first line in {mod_rs_path} to start with `{RULES_SUBMODULE_DOC_PREFIX}`")
            };
        let md_link = comment.trim_end_matches('.');

        let (name, url) = md_link
            .strip_prefix('[')
            .unwrap()
            .strip_suffix(')')
            .unwrap()
            .split_once("](")
            .unwrap();

        let dirname = file.file_name();
        let dirname = dirname.to_str().unwrap();

        let variant_name = dirname
            .split('_')
            .map(|part| match part {
                "errmsg" => "ErrMsg".to_string(),
                "mccabe" => "McCabe".to_string(),
                "pep8" => "PEP8".to_string(),
                _ => format!("{}{}", part[..1].to_uppercase(), &part[1..]),
            })
            .collect::<String>();

        name_match_arms.push_str(&format!(r#"Linter::{variant_name} => "{name}","#));
        url_match_arms.push_str(&format!(r#"Linter::{variant_name} => Some("{url}"),"#));
    }

    write!(
        BufWriter::new(fs::File::create(out_dir.join("linter.rs")).unwrap()),
        "
        impl Linter {{
            pub fn name(&self) -> &'static str {{
                match self {{ {name_match_arms} }}
            }}

            pub fn url(&self) -> Option<&'static str> {{
                match self {{ {url_match_arms} }}
            }}
        }}
        "
    )
    .unwrap();
}
