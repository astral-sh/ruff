//! Build script to package our vendored typeshed files
//! into a zip archive that can be included in the Ruff binary.
//!
//! This script should be automatically run at build time
//! whenever the script itself changes, or whenever any files
//! in `crates/red_knot/vendor/typeshed` change.

use std::fs::File;
use std::path::Path;

use zip::result::ZipResult;
use zip::write::{FileOptions, ZipWriter};
use zip::CompressionMethod;

const TYPESHED_SOURCE_DIR: &str = "vendor/typeshed";

// NB: This is .gitignored; make sure to change the .gitignore entry
// if you change this location
const TYPESHED_ZIP_LOCATION: &str = "vendor/zipped_typeshed.zip";

/// Recursively zip the contents of an entire directory.
///
/// This routine is adapted from a recipe at
/// <https://github.com/zip-rs/zip-old/blob/5d0f198124946b7be4e5969719a7f29f363118cd/examples/write_dir.rs>
fn zip_dir(directory_path: &str, writer: File) -> ZipResult<File> {
    let mut zip = ZipWriter::new(writer);

    let options = FileOptions::default()
        .compression_method(CompressionMethod::Zstd)
        .unix_permissions(0o644);

    for entry in walkdir::WalkDir::new(directory_path) {
        let dir_entry = entry.unwrap();
        let relative_path = dir_entry.path();
        let name = relative_path
            .strip_prefix(Path::new(directory_path))
            .unwrap()
            .to_str()
            .expect("Unexpected non-utf8 typeshed path!");

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if relative_path.is_file() {
            println!("adding file {relative_path:?} as {name:?} ...");
            zip.start_file(name, options)?;
            let mut f = File::open(relative_path)?;
            std::io::copy(&mut f, &mut zip).unwrap();
        } else if !name.is_empty() {
            // Only if not root! Avoids path spec / warning
            // and mapname conversion failed error on unzip
            println!("adding dir {relative_path:?} as {name:?} ...");
            zip.add_directory(name, options)?;
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
    let zipped_typeshed = File::create(Path::new(TYPESHED_ZIP_LOCATION)).unwrap();
    zip_dir(TYPESHED_SOURCE_DIR, zipped_typeshed).unwrap();
}
