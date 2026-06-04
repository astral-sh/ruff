#![warn(
    clippy::disallowed_methods,
    reason = "Prefer System trait methods over std methods in ty crates"
)]
use ruff_db::vendored::VendoredFileSystem;
use std::sync::LazyLock;

/// The source commit of the vendored typeshed.
pub const SOURCE_COMMIT: &str =
    include_str!("../../../crates/ty_vendored/vendor/typeshed/source_commit.txt").trim_ascii_end();

static_assertions::const_assert_eq!(SOURCE_COMMIT.len(), 40);

// The file path here is hardcoded in this crate's `build.rs` script.
// Luckily this crate will fail to build if this file isn't available at build time.
static TYPESHED_ZIP_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/zipped_typeshed.zip"));

// The file path here is hardcoded in this crate's `build.rs` script.
// Luckily this crate will fail to build if this file isn't available at build time.
const VENDORED_CACHE_KEY: &str =
    include_str!(concat!(env!("OUT_DIR"), "/vendored_cache_key.txt")).trim_ascii_end();

/// Returns the version key for the vendored stubs.
///
/// Ruff patches and adds custom files to the vendored typeshed archive. Those
/// files can change independently from the upstream typeshed source commit, so
/// include the final generated vendored content in the cache key.
pub fn cache_key() -> &'static str {
    VENDORED_CACHE_KEY
}

pub fn file_system() -> &'static VendoredFileSystem {
    static VENDORED_TYPESHED_STUBS: LazyLock<VendoredFileSystem> =
        LazyLock::new(|| VendoredFileSystem::new_static(TYPESHED_ZIP_BYTES).unwrap());
    &VENDORED_TYPESHED_STUBS
}

#[cfg(test)]
mod tests {
    use std::io::{self, Read};
    use std::path::Path;

    use ruff_db::vendored::VendoredPath;

    use super::*;

    const TY_EXTENSIONS_STUBS: &str = "ty_extensions/ty_extensions.pyi";
    const TY_EXTENSIONS_ARCHIVE_PATH: &str = "stdlib/ty_extensions.pyi";

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
    fn typeshed_zip_includes_build_time_patches() {
        let mut typeshed_zip_archive =
            zip::ZipArchive::new(io::Cursor::new(TYPESHED_ZIP_BYTES)).unwrap();

        {
            let mut versions = typeshed_zip_archive.by_name("stdlib/VERSIONS").unwrap();
            let mut versions_source = String::new();
            versions.read_to_string(&mut versions_source).unwrap();

            assert!(
                versions_source
                    .lines()
                    .any(|line| line == "ty_extensions: 3.0-")
            );
        }

        {
            let mut ty_extensions = typeshed_zip_archive
                .by_name(TY_EXTENSIONS_ARCHIVE_PATH)
                .unwrap();
            let mut ty_extensions_source = Vec::new();
            ty_extensions
                .read_to_end(&mut ty_extensions_source)
                .unwrap();

            assert_eq!(
                ty_extensions_source,
                std::fs::read(TY_EXTENSIONS_STUBS).unwrap()
            );
        }
    }

    #[test]
    fn cache_key_includes_generated_vendored_content() {
        let generated_content_hash = generated_vendored_content_hash();

        assert_eq!(cache_key(), format!("{generated_content_hash:016x}"));
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

    fn generated_vendored_content_hash() -> u64 {
        let mut typeshed_zip_archive =
            zip::ZipArchive::new(io::Cursor::new(TYPESHED_ZIP_BYTES)).unwrap();
        let mut content_hash = StableContentHasher::new();

        for index in 0..typeshed_zip_archive.len() {
            let mut entry = typeshed_zip_archive.by_index(index).unwrap();
            let archive_path = entry.name().to_string();

            if entry.is_dir() {
                content_hash.add_directory(&archive_path);
            } else {
                let mut contents = Vec::new();
                entry.read_to_end(&mut contents).unwrap();
                content_hash.add_file(&archive_path, &contents);
            }
        }

        content_hash.finish()
    }

    struct StableContentHasher {
        hash: u64,
    }

    impl StableContentHasher {
        fn new() -> Self {
            let mut hasher = Self {
                hash: 0xcbf2_9ce4_8422_2325,
            };
            hasher.update(b"ty-vendored-content-v1");
            hasher
        }

        fn add_file(&mut self, path: &str, contents: &[u8]) {
            self.update(b"file");
            self.update_str(path);
            self.update_len(contents.len());
            self.update(contents);
        }

        fn add_directory(&mut self, path: &str) {
            self.update(b"dir");
            self.update_str(path);
        }

        fn finish(self) -> u64 {
            self.hash
        }

        fn update_str(&mut self, value: &str) {
            self.update_len(value.len());
            self.update(value.as_bytes());
        }

        fn update_len(&mut self, len: usize) {
            let len = u64::try_from(len).expect("vendored content length should fit in u64");
            self.update(&len.to_le_bytes());
        }

        fn update(&mut self, bytes: &[u8]) {
            for byte in bytes {
                self.hash ^= u64::from(*byte);
                self.hash = self.hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
}
