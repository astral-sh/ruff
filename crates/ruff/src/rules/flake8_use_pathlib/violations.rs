use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

// PTH100
#[violation]
pub struct PathlibAbspath;

impl Violation for PathlibAbspath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.abspath()` should be replaced by `Path.resolve()`")
    }
}

// PTH101
#[violation]
pub struct PathlibChmod;

impl Violation for PathlibChmod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.chmod()` should be replaced by `Path.chmod()`")
    }
}

// PTH102
#[violation]
pub struct PathlibMakedirs;

impl Violation for PathlibMakedirs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.makedirs()` should be replaced by `Path.mkdir(parents=True)`")
    }
}

// PTH103
#[violation]
pub struct PathlibMkdir;

impl Violation for PathlibMkdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.mkdir()` should be replaced by `Path.mkdir()`")
    }
}

// PTH104
#[violation]
pub struct PathlibRename;

impl Violation for PathlibRename {
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
pub struct PathlibRmdir;

impl Violation for PathlibRmdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.rmdir()` should be replaced by `Path.rmdir()`")
    }
}

// PTH107
#[violation]
pub struct PathlibRemove;

impl Violation for PathlibRemove {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.remove()` should be replaced by `Path.unlink()`")
    }
}

// PTH108
#[violation]
pub struct PathlibUnlink;

impl Violation for PathlibUnlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.unlink()` should be replaced by `Path.unlink()`")
    }
}

// PTH109
#[violation]
pub struct PathlibGetcwd;

impl Violation for PathlibGetcwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.getcwd()` should be replaced by `Path.cwd()`")
    }
}

// PTH110
#[violation]
pub struct PathlibExists;

impl Violation for PathlibExists {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.exists()` should be replaced by `Path.exists()`")
    }
}

// PTH111
#[violation]
pub struct PathlibExpanduser;

impl Violation for PathlibExpanduser {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.expanduser()` should be replaced by `Path.expanduser()`")
    }
}

// PTH112
#[violation]
pub struct PathlibIsDir;

impl Violation for PathlibIsDir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isdir()` should be replaced by `Path.is_dir()`")
    }
}

// PTH113
#[violation]
pub struct PathlibIsFile;

impl Violation for PathlibIsFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isfile()` should be replaced by `Path.is_file()`")
    }
}

// PTH114
#[violation]
pub struct PathlibIsLink;

impl Violation for PathlibIsLink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.islink()` should be replaced by `Path.is_symlink()`")
    }
}

// PTH115
#[violation]
pub struct PathlibReadlink;

impl Violation for PathlibReadlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.readlink()` should be replaced by `Path.readlink()`")
    }
}

// PTH116
#[violation]
pub struct PathlibStat;

impl Violation for PathlibStat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`"
        )
    }
}

// PTH117
#[violation]
pub struct PathlibIsAbs;

impl Violation for PathlibIsAbs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isabs()` should be replaced by `Path.is_absolute()`")
    }
}

// PTH118
#[violation]
pub struct PathlibJoin;

impl Violation for PathlibJoin {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.join()` should be replaced by `Path` with `/` operator")
    }
}

// PTH119
#[violation]
pub struct PathlibBasename;

impl Violation for PathlibBasename {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.basename()` should be replaced by `Path.name`")
    }
}

// PTH120
#[violation]
pub struct PathlibDirname;

impl Violation for PathlibDirname {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.dirname()` should be replaced by `Path.parent`")
    }
}

// PTH121
#[violation]
pub struct PathlibSamefile;

impl Violation for PathlibSamefile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.samefile()` should be replaced by `Path.samefile()`")
    }
}

// PTH122
#[violation]
pub struct PathlibSplitext;

impl Violation for PathlibSplitext {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.splitext()` should be replaced by `Path.suffix`")
    }
}

// PTH123
#[violation]
pub struct PathlibOpen;

impl Violation for PathlibOpen {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`open()` should be replaced by `Path.open()`")
    }
}

// PTH124
#[violation]
pub struct PathlibPyPath;

impl Violation for PathlibPyPath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`py.path` is in maintenance mode, use `pathlib` instead")
    }
}
