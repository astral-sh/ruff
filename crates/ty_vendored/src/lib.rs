use ruff_db::vendored::VendoredFileSystem;
use std::sync::LazyLock;

// The file path here is hardcoded in this crate's `build.rs` script.
// Luckily this crate will fail to build if this file isn't available at build time.
static TYPESHED_ZIP_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/zipped_typeshed.zip"));

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
