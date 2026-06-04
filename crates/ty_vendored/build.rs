//! Build script to package our vendored typeshed files
//! into a zip archive that can be included in the Ruff binary.
//!
//! This script should be automatically run at build time
//! whenever the script itself changes, or whenever any files
//! in `crates/ty_vendored/vendor/typeshed` or
//! `crates/ty_vendored/ty_extensions` change.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use path_slash::PathExt;
use zip::CompressionMethod;
use zip::result::ZipResult;
use zip::write::{FileOptions, ZipWriter};

const TYPESHED_SOURCE_DIR: &str = "vendor/typeshed";
const TY_EXTENSIONS_STUBS: &str = "ty_extensions/ty_extensions.pyi";
const TY_EXTENSIONS_ARCHIVE_PATH: &str = "stdlib/ty_extensions.pyi";
const TY_EXTENSIONS_VERSION_ENTRY: &[u8] = b"ty_extensions: 3.0-\n";
const TYPESHED_ZIP_LOCATION: &str = "/zipped_typeshed.zip";
const VENDORED_CACHE_KEY_LOCATION: &str = "/vendored_cache_key.txt";

/// Recursively zip the contents of the entire typeshed directory and patch typeshed
/// on the fly to include the `ty_extensions` module.
///
/// This routine is adapted from a recipe at
/// <https://github.com/zip-rs/zip-old/blob/5d0f198124946b7be4e5969719a7f29f363118cd/examples/write_dir.rs>
fn write_zipped_typeshed_to(writer: File) -> ZipResult<u64> {
    let mut zip = ZipWriter::new(writer);
    let mut content_hash = StableContentHasher::new();

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

    for entry in walkdir::WalkDir::new(TYPESHED_SOURCE_DIR).sort_by_file_name() {
        let dir_entry = entry.unwrap();
        let source_path = dir_entry.path();
        let normalized_relative_path = source_path
            .strip_prefix(Path::new(TYPESHED_SOURCE_DIR))
            .unwrap()
            .to_slash()
            .expect("Unexpected non-utf8 typeshed path!");

        if normalized_relative_path.is_empty() {
            continue;
        }

        if source_path.is_file() {
            println!("adding file {source_path:?} as {normalized_relative_path:?} ...");

            let mut contents = fs::read(source_path)?;
            if normalized_relative_path == "stdlib/VERSIONS" {
                contents.extend_from_slice(TY_EXTENSIONS_VERSION_ENTRY);
            }

            zip.start_file(&*normalized_relative_path, options)?;
            zip.write_all(&contents)?;
            content_hash.add_file(&normalized_relative_path, &contents);
        } else {
            // Write directories explicitly. Some unzip tools unzip files with directory
            // paths correctly, and some do not.
            let archive_path = format!("{normalized_relative_path}/");
            println!("adding dir {archive_path:?} ...");
            zip.add_directory(&archive_path, options)?;
            content_hash.add_directory(&archive_path);
        }
    }

    // Patch typeshed and add the stubs for the `ty_extensions` module
    println!("adding file {TY_EXTENSIONS_STUBS} as {TY_EXTENSIONS_ARCHIVE_PATH} ...");
    let ty_extensions = fs::read(TY_EXTENSIONS_STUBS)?;
    zip.start_file(TY_EXTENSIONS_ARCHIVE_PATH, options)?;
    zip.write_all(&ty_extensions)?;
    content_hash.add_file(TY_EXTENSIONS_ARCHIVE_PATH, &ty_extensions);

    zip.finish()?;

    Ok(content_hash.finish())
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

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={TYPESHED_SOURCE_DIR}");
    println!("cargo:rerun-if-changed={TY_EXTENSIONS_STUBS}");

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

    let zipped_typeshed_file = File::create(zipped_typeshed_location).unwrap();
    let content_hash = write_zipped_typeshed_to(zipped_typeshed_file).unwrap();

    let vendored_cache_key_location = format!("{out_dir}{VENDORED_CACHE_KEY_LOCATION}");
    fs::write(
        vendored_cache_key_location,
        format!("{content_hash:016x}\n"),
    )
    .unwrap();
}
