use ruff_macros::derive_message_formats;

use crate::define_simple_violation;
use crate::violation::Violation;

// PTH100
define_simple_violation!(
    PathlibAbspath,
    "`os.path.abspath` should be replaced by `.resolve()`"
);

// PTH101
define_simple_violation!(PathlibChmod, "`os.chmod` should be replaced by `.chmod()`");

// PTH102
define_simple_violation!(
    PathlibMakedirs,
    "`os.makedirs` should be replaced by `.mkdir(parents=True)`"
);

// PTH103
define_simple_violation!(PathlibMkdir, "`os.mkdir` should be replaced by `.mkdir()`");

// PTH104
define_simple_violation!(
    PathlibRename,
    "`os.rename` should be replaced by `.rename()`"
);

// PTH105
define_simple_violation!(
    PathlibReplace,
    "`os.replace`should be replaced by `.replace()`"
);

// PTH106
define_simple_violation!(PathlibRmdir, "`os.rmdir` should be replaced by `.rmdir()`");

// PTH107
define_simple_violation!(
    PathlibRemove,
    "`os.remove` should be replaced by `.unlink()`"
);

// PTH108
define_simple_violation!(
    PathlibUnlink,
    "`os.unlink` should be replaced by `.unlink()`"
);

// PTH109
define_simple_violation!(
    PathlibGetcwd,
    "`os.getcwd` should be replaced by `Path.cwd()`"
);

// PTH110
define_simple_violation!(
    PathlibExists,
    "`os.path.exists` should be replaced by `.exists()`"
);

// PTH111
define_simple_violation!(
    PathlibExpanduser,
    "`os.path.expanduser` should be replaced by `.expanduser()`"
);

// PTH112
define_simple_violation!(
    PathlibIsDir,
    "`os.path.isdir` should be replaced by `.is_dir()`"
);

// PTH113
define_simple_violation!(
    PathlibIsFile,
    "`os.path.isfile` should be replaced by `.is_file()`"
);

// PTH114
define_simple_violation!(
    PathlibIsLink,
    "`os.path.islink` should be replaced by `.is_symlink()`"
);

// PTH115
define_simple_violation!(
    PathlibReadlink,
    "`os.readlink` should be replaced by `.readlink()`"
);

// PTH116
define_simple_violation!(
    PathlibStat,
    "`os.stat` should be replaced by `.stat()` or `.owner()` or `.group()`"
);

// PTH117
define_simple_violation!(
    PathlibIsAbs,
    "`os.path.isabs` should be replaced by `.is_absolute()`"
);

// PTH118
define_simple_violation!(
    PathlibJoin,
    "`os.path.join` should be replaced by foo_path / \"bar\""
);

// PTH119
define_simple_violation!(
    PathlibBasename,
    "`os.path.basename` should be replaced by `.name`"
);

// PTH120
define_simple_violation!(
    PathlibDirname,
    "`os.path.dirname` should be replaced by `.parent`"
);

// PTH121
define_simple_violation!(
    PathlibSamefile,
    "`os.path.samefile` should be replaced by `.samefile()`"
);

// PTH122
define_simple_violation!(
    PathlibSplitext,
    "`os.path.splitext` should be replaced by `.suffix`"
);

// PTH123
define_simple_violation!(
    PathlibOpen,
    "`open(\"foo\")` should be replaced by `Path(\"foo\").open()`"
);

// PTH124
define_simple_violation!(
    PathlibPyPath,
    "`py.path` is in maintenance mode, use `pathlib` instead"
);
