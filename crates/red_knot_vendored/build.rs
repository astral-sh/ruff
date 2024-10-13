//! Build script to package our vendored typeshed files
//! into a zip archive that can be included in the Ruff binary.
//!
//! This script should be automatically run at build time
//! whenever the script itself changes, or whenever any files
//! in `crates/red_knot_vendored/vendor/typeshed` change.

use std::fs::File;
use std::path::Path;

use path_slash::PathExt;
use zip::result::ZipResult;
use zip::write::{FileOptions, ZipWriter};
use zip::CompressionMethod;

const TYPESHED_SOURCE_DIR: &str = "vendor/typeshed";
const TYPESHED_ZIP_LOCATION: &str = "/zipped_typeshed.zip";

/// Recursively zip the contents of an entire directory.
///
/// This routine is adapted from a recipe at
/// <https://github.com/zip-rs/zip-old/blob/5d0f198124946b7be4e5969719a7f29f363118cd/examples/write_dir.rs>
fn zip_dir(directory_path: &str, writer: File) -> ZipResult<File> {
    let mut zip = ZipWriter::new(writer);

    // Use deflated compression for WASM builds because compiling `zstd-sys` requires clang
    // [source](https://github.com/gyscos/zstd-rs/wiki/Compile-for-WASM) which complicates the build
    // by a lot. Deflated compression is slower but it shouldn't matter much for the WASM use case
    // (WASM itself is already slower than a native build for a specific platform).
    // We can't use `#[cfg(...)]` here because the target-arch in a build script is the
    // architecture of the system running the build script and not the architecture of the build-target.
    // That's why we use the `TARGET` environment variable here.
    let method = if cfg!(feature = "zstd") {
        CompressionMethod::Zstd
    } else if cfg!(feature = "deflate") {
        CompressionMethod::Deflated
    } else {
        CompressionMethod::Stored
    };

    let options = FileOptions::default()
        .compression_method(method)
        .unix_permissions(0o644);

    for entry in walkdir::WalkDir::new(directory_path) {
        let dir_entry = entry.unwrap();
        let absolute_path = dir_entry.path();
        let normalized_relative_path = absolute_path
            .strip_prefix(Path::new(directory_path))
            .unwrap()
            .to_slash()
            .expect("Unexpected non-utf8 typeshed path!");

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if absolute_path.is_file() {
            println!("adding file {absolute_path:?} as {normalized_relative_path:?} ...");
            zip.start_file(normalized_relative_path, options)?;
            let mut f = File::open(absolute_path)?;
            std::io::copy(&mut f, &mut zip).unwrap();
        } else if !normalized_relative_path.is_empty() {
            // Only if not root! Avoids path spec / warning
            // and mapname conversion failed error on unzip
            println!("adding dir {absolute_path:?} as {normalized_relative_path:?} ...");
            zip.add_directory(normalized_relative_path, options)?;
        }
    }
    zip.finish()
}

fn main() {
    println!("cargo:rerun-if-changed={TYPESHED_SOURCE_DIR}");
    assert!(
        Path::new(TYPESHED_SOURCE_DIR).is_dir(),
        "Where is typeshed?"
    );
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // N.B. Deliberately using `format!()` instead of `Path::join()` here,
    // so that we use `/` as a path separator on all platforms.
    // That enables us to load the typeshed zip at compile time in `module.rs`
    // (otherwise we'd have to dynamically determine the exact path to the typeshed zip
    // based on the default path separator for the specific platform we're on,
    // which can't be done at compile time.)
    let zipped_typeshed_location = format!("{out_dir}{TYPESHED_ZIP_LOCATION}");

    let zipped_typeshed = File::create(zipped_typeshed_location).unwrap();
    zip_dir(TYPESHED_SOURCE_DIR, zipped_typeshed).unwrap();
}
