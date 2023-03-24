use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

// PTH100
#[violation]
pub struct OsPathAbspath;

impl Violation for OsPathAbspath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.abspath()` should be replaced by `Path.resolve()`")
    }
}

// PTH101
#[violation]
pub struct OsChmod;

impl Violation for OsChmod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.chmod()` should be replaced by `Path.chmod()`")
    }
}

// PTH102
#[violation]
pub struct OsMakedirs;

impl Violation for OsMakedirs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.makedirs()` should be replaced by `Path.mkdir(parents=True)`")
    }
}

// PTH103
#[violation]
pub struct OsMkdir;

impl Violation for OsMkdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.mkdir()` should be replaced by `Path.mkdir()`")
    }
}

// PTH104
#[violation]
pub struct OsRename;

impl Violation for OsRename {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.rename()` should be replaced by `Path.rename()`")
    }
}

// PTH105
#[violation]
pub struct PathlibReplace;

impl Violation for PathlibReplace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.replace()` should be replaced by `Path.replace()`")
    }
}

// PTH106
#[violation]
pub struct OsRmdir;

impl Violation for OsRmdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.rmdir()` should be replaced by `Path.rmdir()`")
    }
}

// PTH107
#[violation]
pub struct OsRemove;

impl Violation for OsRemove {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.remove()` should be replaced by `Path.unlink()`")
    }
}

// PTH108
#[violation]
pub struct OsUnlink;

impl Violation for OsUnlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.unlink()` should be replaced by `Path.unlink()`")
    }
}

// PTH109
#[violation]
pub struct OsGetcwd;

impl Violation for OsGetcwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.getcwd()` should be replaced by `Path.cwd()`")
    }
}

// PTH110
#[violation]
pub struct OsPathExists;

impl Violation for OsPathExists {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.exists()` should be replaced by `Path.exists()`")
    }
}

// PTH111
#[violation]
pub struct OsPathExpanduser;

impl Violation for OsPathExpanduser {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.expanduser()` should be replaced by `Path.expanduser()`")
    }
}

// PTH112
#[violation]
pub struct OsPathIsdir;

impl Violation for OsPathIsdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isdir()` should be replaced by `Path.is_dir()`")
    }
}

// PTH113
#[violation]
pub struct OsPathIsfile;

impl Violation for OsPathIsfile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isfile()` should be replaced by `Path.is_file()`")
    }
}

// PTH114
#[violation]
pub struct OsPathIslink;

impl Violation for OsPathIslink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.islink()` should be replaced by `Path.is_symlink()`")
    }
}

// PTH115
#[violation]
pub struct OsReadlink;

impl Violation for OsReadlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.readlink()` should be replaced by `Path.readlink()`")
    }
}

// PTH116
#[violation]
pub struct OsStat;

impl Violation for OsStat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`"
        )
    }
}

// PTH117
#[violation]
pub struct OsPathIsabs;

impl Violation for OsPathIsabs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isabs()` should be replaced by `Path.is_absolute()`")
    }
}

// PTH118
#[violation]
pub struct OsPathJoin;

impl Violation for OsPathJoin {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.join()` should be replaced by `Path` with `/` operator")
    }
}

// PTH119
#[violation]
pub struct OsPathBasename;

impl Violation for OsPathBasename {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.basename()` should be replaced by `Path.name`")
    }
}

// PTH120
#[violation]
pub struct OsPathDirname;

impl Violation for OsPathDirname {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.dirname()` should be replaced by `Path.parent`")
    }
}

// PTH121
#[violation]
pub struct OsPathSamefile;

impl Violation for OsPathSamefile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.samefile()` should be replaced by `Path.samefile()`")
    }
}

// PTH122
#[violation]
pub struct OsPathSplitext;

impl Violation for OsPathSplitext {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.splitext()` should be replaced by `Path.suffix`")
    }
}

// PTH123
#[violation]
pub struct BuiltinOpen;

impl Violation for BuiltinOpen {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`open()` should be replaced by `Path.open()`")
    }
}

// PTH124
#[violation]
pub struct PyPath;

impl Violation for PyPath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`py.path` is in maintenance mode, use `pathlib` instead")
    }
}
