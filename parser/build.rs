use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use tiny_keccak::{Hasher, Sha3};

fn main() -> anyhow::Result<()> {
    const SOURCE: &str = "python.lalrpop";
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed={SOURCE}");

    try_lalrpop(SOURCE, &out_dir.join("python.rs"))?;
    gen_phf(&out_dir);

    Ok(())
}

fn requires_lalrpop(source: &str, target: &Path) -> Option<String> {
    let Ok(target) = File::open(target) else {
        return Some("python.rs doesn't exist. regenerate.".to_owned());
    };

    let sha_prefix = "// sha3: ";
    let sha3_line = if let Some(sha3_line) =
        BufReader::with_capacity(128, target)
            .lines()
            .find_map(|line| {
                let line = line.unwrap();
                line.starts_with(sha_prefix).then_some(line)
            }) {
        sha3_line
    } else {
        // no sha3 line - maybe old version of lalrpop installed
        return Some("python.rs doesn't include sha3 hash. regenerate.".to_owned());
    };
    let expected_sha3_str = sha3_line.strip_prefix(sha_prefix).unwrap();

    let actual_sha3 = {
        let mut hasher = Sha3::v256();
        let mut f = BufReader::new(File::open(source).unwrap());
        let mut line = String::new();
        while f.read_line(&mut line).unwrap() != 0 {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            hasher.update(line.as_bytes());
            hasher.update(b"\n");
            line.clear();
        }
        let mut hash = [0u8; 32];
        hasher.finalize(&mut hash);
        hash
    };
    let eq = sha_equal(expected_sha3_str, &actual_sha3);
    if !eq {
        let mut actual_sha3_str = String::new();
        for byte in actual_sha3 {
            write!(actual_sha3_str, "{byte:02x}").unwrap();
        }
        return Some(format!(
            "python.rs hash expected: {expected_sha3_str} but actual: {actual_sha3_str}"
        ));
    }
    None
}

fn try_lalrpop(source: &str, target: &Path) -> anyhow::Result<()> {
    let Some(_message) = requires_lalrpop(source, target) else {
        return Ok(());
    };

    #[cfg(feature = "lalrpop")]
    // We are not using lalrpop::process_root() or Configuration::process_current_dir()
    // because of https://github.com/lalrpop/lalrpop/issues/699.
    lalrpop::Configuration::new()
        .use_cargo_dir_conventions()
        .set_in_dir(Path::new("."))
        .process()
        .unwrap_or_else(|e| {
            println!("cargo:warning={_message}");
            panic!("running lalrpop failed. {e:?}");
        });

    #[cfg(not(feature = "lalrpop"))]
    {
        println!("cargo:warning=try: cargo build --manifest-path=compiler/parser/Cargo.toml --features=lalrpop");
    }
    Ok(())
}

fn sha_equal(expected_sha3_str: &str, actual_sha3: &[u8; 32]) -> bool {
    if expected_sha3_str.len() != 64 {
        panic!("lalrpop version? hash bug is fixed in 0.19.8");
    }

    let mut expected_sha3 = [0u8; 32];
    for (i, b) in expected_sha3.iter_mut().enumerate() {
        *b = u8::from_str_radix(&expected_sha3_str[i * 2..][..2], 16).unwrap();
    }
    *actual_sha3 == expected_sha3
}

fn gen_phf(out_dir: &Path) {
    let mut kwds = phf_codegen::Map::new();
    let kwds = kwds
        // Alphabetical keywords:
        .entry("...", "Tok::Ellipsis")
        .entry("False", "Tok::False")
        .entry("None", "Tok::None")
        .entry("True", "Tok::True")
        // moreso "standard" keywords
        .entry("and", "Tok::And")
        .entry("as", "Tok::As")
        .entry("assert", "Tok::Assert")
        .entry("async", "Tok::Async")
        .entry("await", "Tok::Await")
        .entry("break", "Tok::Break")
        .entry("class", "Tok::Class")
        .entry("continue", "Tok::Continue")
        .entry("def", "Tok::Def")
        .entry("del", "Tok::Del")
        .entry("elif", "Tok::Elif")
        .entry("else", "Tok::Else")
        .entry("except", "Tok::Except")
        .entry("finally", "Tok::Finally")
        .entry("for", "Tok::For")
        .entry("from", "Tok::From")
        .entry("global", "Tok::Global")
        .entry("if", "Tok::If")
        .entry("import", "Tok::Import")
        .entry("in", "Tok::In")
        .entry("is", "Tok::Is")
        .entry("lambda", "Tok::Lambda")
        .entry("nonlocal", "Tok::Nonlocal")
        .entry("not", "Tok::Not")
        .entry("or", "Tok::Or")
        .entry("pass", "Tok::Pass")
        .entry("raise", "Tok::Raise")
        .entry("return", "Tok::Return")
        .entry("try", "Tok::Try")
        .entry("while", "Tok::While")
        .entry("with", "Tok::With")
        .entry("yield", "Tok::Yield")
        .build();
    writeln!(
        BufWriter::new(File::create(out_dir.join("keywords.rs")).unwrap()),
        "{kwds}",
    )
    .unwrap();
}
