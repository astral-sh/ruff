use ruff_macros::{define_violation, derive_message_formats};

use crate::violation::Violation;

// PTH100
define_violation!(
    pub struct PathlibAbspath;
);
impl Violation for PathlibAbspath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.abspath` should be replaced by `.resolve()`")
    }
}

// PTH101
define_violation!(
    pub struct PathlibChmod;
);
impl Violation for PathlibChmod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.chmod` should be replaced by `.chmod()`")
    }
}

// PTH102
define_violation!(
    pub struct PathlibMakedirs;
);
impl Violation for PathlibMakedirs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.makedirs` should be replaced by `.mkdir(parents=True)`")
    }
}

// PTH103
define_violation!(
    pub struct PathlibMkdir;
);
impl Violation for PathlibMkdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.mkdir` should be replaced by `.mkdir()`")
    }
}

// PTH104
define_violation!(
    pub struct PathlibRename;
);
impl Violation for PathlibRename {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.rename` should be replaced by `.rename()`")
    }
}

// PTH105
define_violation!(
    pub struct PathlibReplace;
);
impl Violation for PathlibReplace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.replace`should be replaced by `.replace()`")
    }
}

// PTH106
define_violation!(
    pub struct PathlibRmdir;
);
impl Violation for PathlibRmdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.rmdir` should be replaced by `.rmdir()`")
    }
}

// PTH107
define_violation!(
    pub struct PathlibRemove;
);
impl Violation for PathlibRemove {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.remove` should be replaced by `.unlink()`")
    }
}

// PTH108
define_violation!(
    pub struct PathlibUnlink;
);
impl Violation for PathlibUnlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.unlink` should be replaced by `.unlink()`")
    }
}

// PTH109
define_violation!(
    pub struct PathlibGetcwd;
);
impl Violation for PathlibGetcwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.getcwd` should be replaced by `Path.cwd()`")
    }
}

// PTH110
define_violation!(
    pub struct PathlibExists;
);
impl Violation for PathlibExists {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.exists` should be replaced by `.exists()`")
    }
}

// PTH111
define_violation!(
    pub struct PathlibExpanduser;
);
impl Violation for PathlibExpanduser {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.expanduser` should be replaced by `.expanduser()`")
    }
}

// PTH112
define_violation!(
    pub struct PathlibIsDir;
);
impl Violation for PathlibIsDir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isdir` should be replaced by `.is_dir()`")
    }
}

// PTH113
define_violation!(
    pub struct PathlibIsFile;
);
impl Violation for PathlibIsFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isfile` should be replaced by `.is_file()`")
    }
}

// PTH114
define_violation!(
    pub struct PathlibIsLink;
);
impl Violation for PathlibIsLink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.islink` should be replaced by `.is_symlink()`")
    }
}

// PTH115
define_violation!(
    pub struct PathlibReadlink;
);
impl Violation for PathlibReadlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.readlink` should be replaced by `.readlink()`")
    }
}

// PTH116
define_violation!(
    pub struct PathlibStat;
);
impl Violation for PathlibStat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.stat` should be replaced by `.stat()` or `.owner()` or `.group()`")
    }
}

// PTH117
define_violation!(
    pub struct PathlibIsAbs;
);
impl Violation for PathlibIsAbs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isabs` should be replaced by `.is_absolute()`")
    }
}

// PTH118
define_violation!(
    pub struct PathlibJoin;
);
impl Violation for PathlibJoin {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.join` should be replaced by foo_path / \"bar\"")
    }
}

// PTH119
define_violation!(
    pub struct PathlibBasename;
);
impl Violation for PathlibBasename {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.basename` should be replaced by `.name`")
    }
}

// PTH120
define_violation!(
    pub struct PathlibDirname;
);
impl Violation for PathlibDirname {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.dirname` should be replaced by `.parent`")
    }
}

// PTH121
define_violation!(
    pub struct PathlibSamefile;
);
impl Violation for PathlibSamefile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.samefile` should be replaced by `.samefile()`")
    }
}

// PTH122
define_violation!(
    pub struct PathlibSplitext;
);
impl Violation for PathlibSplitext {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.splitext` should be replaced by `.suffix`")
    }
}

// PTH123
define_violation!(
    pub struct PathlibOpen;
);
impl Violation for PathlibOpen {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`open(\"foo\")` should be replaced by `Path(\"foo\").open()`")
    }
}

// PTH124
define_violation!(
    pub struct PathlibPyPath;
);
impl Violation for PathlibPyPath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`py.path` is in maintenance mode, use `pathlib` instead")
    }
}
