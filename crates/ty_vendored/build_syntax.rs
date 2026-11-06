use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use path_slash::PathExt;
use ruff_python_ast::{PySourceType, PythonVersion};
use ruff_python_parser::{ParseOptions, parse_unchecked};
use walkdir::WalkDir;

use super::{TY_EXTENSIONS_STUBS, TYPESHED_SOURCE_DIR};

pub(super) fn write(out: &Path) {
    let mut files = BTreeMap::<String, PathBuf>::new();
    let root = Path::new(TYPESHED_SOURCE_DIR);
    for entry in WalkDir::new(root) {
        let entry = entry.unwrap();
        let file = entry.path();
        if file.extension().is_some_and(|ext| ext == "pyi") {
            files.insert(
                file.strip_prefix(root)
                    .unwrap()
                    .to_slash()
                    .unwrap()
                    .into_owned(),
                file.to_owned(),
            );
        }
    }
    for &(file, name) in TY_EXTENSIONS_STUBS {
        files.insert(name.to_owned(), PathBuf::from(file));
    }

    let mut bytes = Vec::new();
    let mut index = String::from("&[\n");
    for (name, path) in files {
        let source = fs::read_to_string(path).unwrap();
        let mut locations = Vec::new();
        let mut unique = Vec::<(u32, u32)>::new();
        for version in PythonVersion::iter() {
            let parsed = parse_unchecked(
                &source,
                ParseOptions::from(PySourceType::Stub).with_target_version(version),
            )
            .try_into_module()
            .expect("a stub is parsed in module mode");
            let Some(encoded) = parsed.encode_stub() else {
                locations.push((0u32, 0u32));
                continue;
            };
            let location = unique
                .iter()
                .find(|&&(start, len)| bytes[start as usize..(start + len) as usize] == encoded)
                .copied()
                .unwrap_or_else(|| {
                    let (round_trip, token_image) = ruff_python_ast::stub_archive::decode(&encoded)
                        .expect("bundled syntax should round trip");
                    assert!(
                        &round_trip == parsed.syntax(),
                        "syntax differs for {name} in Python {version}"
                    );
                    let tokens = ruff_python_ast::stub_archive::decode_tokens(token_image)
                        .expect("bundled tokens should round trip");
                    assert!(
                        &tokens == parsed.tokens(),
                        "tokens differ for {name} in Python {version}"
                    );
                    let range = (
                        u32::try_from(bytes.len()).unwrap(),
                        u32::try_from(encoded.len()).unwrap(),
                    );
                    bytes.extend_from_slice(&encoded);
                    unique.push(range);
                    range
                });
            locations.push(location);
        }
        if !unique.is_empty() {
            writeln!(index, "({name:?}, &{locations:?}),").unwrap();
        }
    }
    index.push_str("]\n");
    fs::write(out.join("stub-syntax.rs"), index).unwrap();
    fs::write(out.join("stub-syntax.bin"), bytes).unwrap();
}
