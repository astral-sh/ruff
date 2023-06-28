//! Support for native Python extension modules.

use std::ffi::OsStr;
use std::path::Path;

/// Returns `true` if the given file extension is that of a native module.
pub(crate) fn is_native_module_file_extension(file_extension: &OsStr) -> bool {
    file_extension == "so" || file_extension == "pyd" || file_extension == "dylib"
}

/// Returns `true` if the given file name is that of a native module.
pub(crate) fn is_native_module_file_name(_module_name: &Path, _file_name: &Path) -> bool {
    todo!()
}
