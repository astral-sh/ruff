//! Build script to package our vendored typeshed files
//! into a zip archive that can be included in the Ruff binary.
//!
//! This script should be automatically run at build time
//! whenever the script itself changes, or whenever any files
//! in `crates/red_knot/vendor/typeshed` change.

use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use walkdir::{DirEntry, WalkDir};
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
fn zip_dir(
    directory_iterator: &mut impl Iterator<Item = DirEntry>,
    prefix: &str,
    writer: File,
) -> ZipResult<()> {
    let mut zip = ZipWriter::new(writer);

    let options = FileOptions::default()
        .compression_method(CompressionMethod::Zstd)
        .unix_permissions(0o644);

    let mut buffer = Vec::new();
    for entry in directory_iterator {
        let path = entry.path();
        let name = path
            .strip_prefix(Path::new(prefix))
            .unwrap()
            .to_str()
            .expect("Unexpected non-utf8 typeshed path!");

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if path.is_file() {
            println!("adding file {path:?} as {name:?} ...");
            zip.start_file(name, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.is_empty() {
            // Only if not root! Avoids path spec / warning
            // and mapname conversion failed error on unzip
            println!("adding dir {path:?} as {name:?} ...");

            zip.add_directory(name, options)?;
        }
    }
    zip.finish()?;
    Result::Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={TYPESHED_SOURCE_DIR}");

    assert!(
        Path::new(TYPESHED_SOURCE_DIR).is_dir(),
        "Where is typeshed?"
    );

    let zipped_typeshed = File::create(Path::new(TYPESHED_ZIP_LOCATION)).unwrap();

    let mut typeshed_traverser = WalkDir::new(TYPESHED_SOURCE_DIR)
        .into_iter()
        .filter_map(std::result::Result::ok);

    zip_dir(
        &mut typeshed_traverser,
        TYPESHED_SOURCE_DIR,
        zipped_typeshed,
    )?;
    Ok(())
}
