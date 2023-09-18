//! Support for native Python extension modules.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

/// Returns `true` if the given file extension is that of a native module.
pub(crate) fn is_native_module_file_extension(file_extension: &OsStr) -> bool {
    file_extension == "so" || file_extension == "pyd" || file_extension == "dylib"
}

/// Given a file name, returns the name of the native module it represents.
///
/// For example, given `foo.abi3.so`, return `foo`.
pub(crate) fn native_module_name(file_name: &Path) -> Option<&str> {
    file_name
        .file_stem()
        .and_then(OsStr::to_str)
        .map(|file_stem| {
            file_stem
                .split_once('.')
                .map_or(file_stem, |(file_stem, _)| file_stem)
        })
}

/// Returns `true` if the given file name is that of a native module with the given name.
pub(crate) fn is_native_module_file_name(module_name: &str, file_name: &Path) -> bool {
    // The file name must be that of a native module.
    if !file_name
        .extension()
        .is_some_and(is_native_module_file_extension)
    {
        return false;
    };

    // The file must represent the module name.
    native_module_name(file_name) == Some(module_name)
}

/// Find the native module within the namespace package at the given path.
pub(crate) fn find_native_module(
    module_name: &str,
    dir_path: &Path,
) -> io::Result<Option<PathBuf>> {
    Ok(dir_path
        .read_dir()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|ft| ft.is_file()))
        .map(|entry| entry.path())
        .find(|path| is_native_module_file_name(module_name, path)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[test]
    fn module_name() {
        assert_eq!(
            super::native_module_name(&PathBuf::from("foo.so")),
            Some("foo")
        );

        assert_eq!(
            super::native_module_name(&PathBuf::from("foo.abi3.so")),
            Some("foo")
        );

        assert_eq!(
            super::native_module_name(&PathBuf::from("foo.cpython-38-x86_64-linux-gnu.so")),
            Some("foo")
        );

        assert_eq!(
            super::native_module_name(&PathBuf::from("foo.cp39-win_amd64.pyd")),
            Some("foo")
        );
    }

    #[test]
    fn module_file_extension() {
        assert!(super::is_native_module_file_extension("so".as_ref()));
        assert!(super::is_native_module_file_extension("pyd".as_ref()));
        assert!(super::is_native_module_file_extension("dylib".as_ref()));
        assert!(!super::is_native_module_file_extension("py".as_ref()));
    }
}
