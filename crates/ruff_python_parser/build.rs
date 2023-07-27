use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tiny_keccak::{Hasher, Sha3};

fn main() {
    const SOURCE: &str = "src/python.lalrpop";
    println!("cargo:rerun-if-changed={SOURCE}");

    let target;
    let error;

    #[cfg(feature = "lalrpop")]
    {
        let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        target = out_dir.join("src/python.rs");
    }
    #[cfg(not(feature = "lalrpop"))]
    {
        target = PathBuf::from("src/python.rs");
        error = "python.lalrpop and src/python.rs doesn't match. This is a ruff_python_parser bug. Please report it unless you are editing ruff_python_parser. Run `lalrpop src/python.lalrpop` to build ruff_python_parser again.";
    }

    let Some(message) = requires_lalrpop(SOURCE, &target) else {
        return;
    };

    #[cfg(feature = "lalrpop")]
    {
        let Err(e) = try_lalrpop() else {
            return;
        };
        error = e;
    }

    println!("cargo:warning={message}");
    panic!("running lalrpop failed. {error:?}");
}

fn requires_lalrpop(source: &str, target: &Path) -> Option<String> {
    let Ok(target) = File::open(target) else {
        return Some("python.rs doesn't exist. regenerate.".to_owned());
    };

    let sha_prefix = "// sha3: ";
    let Some(sha3_line) = BufReader::with_capacity(128, target)
        .lines()
        .find_map(|line| {
            let line = line.unwrap();
            line.starts_with(sha_prefix).then_some(line)
        })
    else {
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

#[cfg(feature = "lalrpop")]
fn try_lalrpop() -> Result<(), Box<dyn std::error::Error>> {
    // We are not using lalrpop::process_root() or Configuration::process_current_dir()
    // because of https://github.com/lalrpop/lalrpop/issues/699.
    lalrpop::Configuration::new()
        .use_cargo_dir_conventions()
        .set_in_dir(Path::new("."))
        .process()
}

fn sha_equal(expected_sha3_str: &str, actual_sha3: &[u8; 32]) -> bool {
    assert!(
        expected_sha3_str.len() == 64,
        "lalrpop version? hash bug is fixed in 0.19.8"
    );

    let mut expected_sha3 = [0u8; 32];
    for (i, b) in expected_sha3.iter_mut().enumerate() {
        *b = u8::from_str_radix(&expected_sha3_str[i * 2..][..2], 16).unwrap();
    }
    *actual_sha3 == expected_sha3
}
