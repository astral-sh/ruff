#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
use ruff_db::vendored::VendoredFileSystem;
use std::sync::LazyLock;

/// The source commit of the vendored typeshed.
pub const SOURCE_COMMIT: &str =
    include_str!("../vendor/typeshed/source_commit.txt").trim_ascii_end();

static_assertions::const_assert_eq!(SOURCE_COMMIT.len(), 40);

// The file path here is hardcoded in this crate's `build.rs` script.
// Luckily this crate will fail to build if this file isn't available at build time.
static TYPESHED_ZIP_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/zipped_typeshed.zip"));

#[cfg(feature = "bundled-syntax")]
#[expect(
    clippy::unreadable_literal,
    reason = "the generated table contains byte offsets and lengths within the syntax image"
)]
static STUB_SYNTAX_INDEX: &[(&str, &[(u32, u32)])] =
    include!(concat!(env!("OUT_DIR"), "/stub-syntax.rs"));

#[cfg(feature = "bundled-syntax")]
fn stub_syntax(
    path: &ruff_db::vendored::VendoredPath,
    version: ruff_python_ast::PythonVersion,
) -> Option<ruff_python_parser::Parsed<ruff_python_ast::ModModule>> {
    static SYNTAX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stub-syntax.bin"));
    let index = STUB_SYNTAX_INDEX
        .binary_search_by_key(&path.as_str(), |(name, _)| name)
        .ok()?;
    let position = ruff_python_ast::PythonVersion::iter().position(|target| target == version)?;
    let &(start, len) = STUB_SYNTAX_INDEX[index].1.get(position)?;
    if len == 0 {
        return None;
    }
    ruff_python_parser::Parsed::decode_packaged_stub(
        SYNTAX.get(start as usize..(start + len) as usize)?,
    )
}

pub fn file_system() -> &'static VendoredFileSystem {
    static VENDORED_TYPESHED_STUBS: LazyLock<VendoredFileSystem> = LazyLock::new(|| {
        let fs = VendoredFileSystem::new_static(TYPESHED_ZIP_BYTES).unwrap();
        #[cfg(feature = "bundled-syntax")]
        let fs = fs.with_stub_syntax(stub_syntax);
        fs
    });
    &VENDORED_TYPESHED_STUBS
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read};
    use std::path::Path;

    use ruff_db::vendored::VendoredPath;

    use super::*;

    #[cfg(feature = "bundled-syntax")]
    #[test]
    fn packaged_syntax_matches_the_bundled_source() {
        use ruff_python_ast::{PySourceType, PythonVersion};
        use ruff_python_parser::{ParseOptions, parse_unchecked};
        let path = VendoredPath::new("stdlib/builtins.pyi");
        let source = file_system().read_to_string(path).unwrap();
        for version in [
            PythonVersion::PY39,
            PythonVersion::PY311,
            PythonVersion::PY315,
        ] {
            let parsed = parse_unchecked(
                &source,
                ParseOptions::from(PySourceType::Stub).with_target_version(version),
            )
            .try_into_module()
            .unwrap();
            let packaged = stub_syntax(path, version).unwrap();
            assert_eq!(packaged.syntax(), parsed.syntax());
            assert_eq!(packaged.tokens(), parsed.tokens());
            assert_eq!(
                packaged.unsupported_syntax_errors(),
                parsed.unsupported_syntax_errors()
            );
        }
        assert!(
            stub_syntax(
                VendoredPath::new("stdlib/missing.pyi"),
                PythonVersion::PY311
            )
            .is_none()
        );
    }

    #[cfg(feature = "bundled-syntax")]
    #[test]
    #[ignore = "compares the complete bundled corpus across all supported target versions"]
    fn every_packaged_stub_matches_the_parser() {
        use ruff_python_ast::{PySourceType, PythonVersion};
        use ruff_python_parser::{ParseOptions, parse_unchecked};
        let mut comparisons = 0;
        for &(name, records) in STUB_SYNTAX_INDEX {
            let path = VendoredPath::new(name);
            let source = file_system().read_to_string(path).unwrap();
            for (version, &(_, len)) in PythonVersion::iter().zip(records) {
                let packaged = stub_syntax(path, version);
                if len == 0 {
                    assert!(packaged.is_none(), "{name} in Python {version}");
                    continue;
                }
                let expected = parse_unchecked(
                    &source,
                    ParseOptions::from(PySourceType::Stub).with_target_version(version),
                )
                .try_into_module()
                .unwrap();
                let actual = packaged.unwrap();
                assert!(
                    actual.syntax() == expected.syntax(),
                    "syntax of {name} in Python {version}"
                );
                assert!(
                    actual.tokens() == expected.tokens(),
                    "tokens of {name} in Python {version}"
                );
                assert!(
                    actual.has_no_syntax_errors() && expected.has_no_syntax_errors(),
                    "{name} in Python {version}"
                );
                comparisons += 1;
            }
        }
        assert!(comparisons > 0, "expected at least one packaged stub");
    }

    #[cfg(feature = "bundled-syntax")]
    #[test]
    fn syntax_errors_and_unrepresented_statements_use_the_parser() {
        use ruff_python_ast::PySourceType;
        use ruff_python_parser::{ParseOptions, parse_unchecked};
        for source in ["x: int =\n", "assert x\n"] {
            let parsed = parse_unchecked(source, ParseOptions::from(PySourceType::Stub))
                .try_into_module()
                .unwrap();
            assert!(parsed.encode_stub().is_none());
        }
    }

    #[test]
    fn typeshed_zip_created_at_build_time() {
        let mut typeshed_zip_archive =
            zip::ZipArchive::new(io::Cursor::new(TYPESHED_ZIP_BYTES)).unwrap();

        let mut functools_module_stub = typeshed_zip_archive
            .by_name("stdlib/functools.pyi")
            .unwrap();
        assert!(functools_module_stub.is_file());

        let mut functools_module_stub_source = String::new();
        functools_module_stub
            .read_to_string(&mut functools_module_stub_source)
            .unwrap();

        assert!(functools_module_stub_source.contains("def update_wrapper("));
    }

    #[test]
    fn typeshed_vfs_consistent_with_vendored_stubs() {
        let vendored_typeshed_dir = Path::new("vendor/typeshed").canonicalize().unwrap();
        let vendored_typeshed_stubs = file_system();

        let mut empty_iterator = true;
        for entry in walkdir::WalkDir::new(&vendored_typeshed_dir).min_depth(1) {
            empty_iterator = false;
            let entry = entry.unwrap();
            let absolute_path = entry.path();
            let file_type = entry.file_type();

            let relative_path = absolute_path
                .strip_prefix(&vendored_typeshed_dir)
                .unwrap_or_else(|_| {
                    panic!("Expected {absolute_path:?} to be a child of {vendored_typeshed_dir:?}")
                });

            let vendored_path = <&VendoredPath>::try_from(relative_path)
                .unwrap_or_else(|_| panic!("Expected {relative_path:?} to be valid UTF-8"));

            assert!(
                vendored_typeshed_stubs.exists(vendored_path),
                "Expected {vendored_path:?} to exist in the `VendoredFileSystem`!

                Vendored file system:

                {vendored_typeshed_stubs:#?}
                "
            );

            let vendored_path_kind = vendored_typeshed_stubs
                .metadata(vendored_path)
                .unwrap_or_else(|_| {
                    panic!(
                        "Expected metadata for {vendored_path:?} to be retrievable from the `VendoredFileSystem!

                        Vendored file system:

                        {vendored_typeshed_stubs:#?}
                        "
                    )
                })
                .kind();

            assert_eq!(
                vendored_path_kind.is_directory(),
                file_type.is_dir(),
                "{vendored_path:?} had type {vendored_path_kind:?}, inconsistent with fs path {relative_path:?}: {file_type:?}"
            );
        }

        assert!(
            !empty_iterator,
            "Expected there to be at least one file or directory in the vendored typeshed stubs!"
        );
    }
}
