//! Build script to package our vendored typeshed files
//! into a static index and data blob that can be included in the binary.
//!
//! This script should be automatically run at build time
//! whenever the script itself changes, or whenever any files
//! in `crates/ty_vendored/vendor/typeshed` change.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use path_slash::PathExt;

const TYPESHED_SOURCE_DIR: &str = "vendor/typeshed";
const TY_EXTENSIONS_STUBS: &[(&str, &str)] = &[
    (
        "ty_extensions/__init__.pyi",
        "stdlib/ty_extensions/__init__.pyi",
    ),
    (
        "ty_extensions/_internal.pyi",
        "stdlib/ty_extensions/_internal.pyi",
    ),
    (
        "ty_extensions/pydantic.pyi",
        "stdlib/ty_extensions/pydantic.pyi",
    ),
];

fn compress(contents: &[u8]) -> io::Result<Vec<u8>> {
    #[cfg(feature = "zstd")]
    {
        zstd::bulk::compress(contents, zstd::DEFAULT_COMPRESSION_LEVEL)
    }
    // WASM uses a Rust Deflate implementation so building the playground does not require
    // a C toolchain for `zstd-sys`.
    #[cfg(all(not(feature = "zstd"), feature = "deflate"))]
    {
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(contents)?;
        encoder.finish()
    }
    #[cfg(not(any(feature = "zstd", feature = "deflate")))]
    {
        Ok(contents.to_vec())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut paths: BTreeMap<String, Option<PathBuf>> = BTreeMap::new();
    for entry in walkdir::WalkDir::new(TYPESHED_SOURCE_DIR).min_depth(1) {
        let entry = entry?;
        let path = entry.path();
        let relative = path
            .strip_prefix(Path::new(TYPESHED_SOURCE_DIR))?
            .to_slash()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 typeshed path"))?
            .into_owned();
        paths.insert(relative, path.is_file().then(|| path.to_owned()));
    }

    // Patch typeshed and add the stubs for the `ty_extensions` package.
    paths.insert("stdlib/ty_extensions".to_owned(), None);
    for (source, destination) in TY_EXTENSIONS_STUBS {
        if paths
            .insert((*destination).to_owned(), Some(PathBuf::from(source)))
            .is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "duplicate ty_extensions stub",
            )
            .into());
        }
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let mut data = BufWriter::new(File::create(out_dir.join("typeshed.bin"))?);
    let mut index = BufWriter::new(File::create(out_dir.join("typeshed_index.rs"))?);
    let mut offset = 0usize;
    writeln!(index, "&[")?;
    for (path, source) in paths {
        if let Some(source) = source {
            println!("adding file {source:?} as {path:?} ...");
            let mut contents = fs::read(source)?;
            // Patch the VERSIONS file to make `ty_extensions` available
            if path == "stdlib/VERSIONS" {
                contents.extend_from_slice(b"ty_extensions: 3.0-\n");
            }
            let compressed = compress(&contents)?;
            let end = offset.checked_add(compressed.len()).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "vendored contents are too large",
                )
            })?;
            writeln!(
                index,
                "    VendoredEntry::file({path:?}, {offset}..{end}, {}, {}),",
                contents.len(),
                crc32fast::hash(&contents),
            )?;
            data.write_all(&compressed)?;
            offset = end;
        } else {
            println!("adding dir {path:?} ...");
            writeln!(index, "    VendoredEntry::directory({path:?}),")?;
        }
    }
    writeln!(index, "]")?;
    index.flush()?;
    data.flush()?;
    Ok(())
}
