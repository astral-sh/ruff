use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

/// ## What it does
/// Checks for uses of `os.makedirs`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.mkdir(parents=True)` can improve readability over the
/// `os` module's counterparts (e.g., `os.makedirs()`.
///
/// ## Examples
/// ```python
/// import os
///
/// os.makedirs("./nested/directory/")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("./nested/directory/").mkdir(parents=True)
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.mkdir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.mkdir)
/// - [Python documentation: `os.makedirs`](https://docs.python.org/3/library/os.html#os.makedirs)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsMakedirs;

impl Violation for OsMakedirs {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.makedirs()` should be replaced by `Path.mkdir(parents=True)`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.mkdir`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.mkdir()` can improve readability over the `os`
/// module's counterparts (e.g., `os.mkdir()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.mkdir("./directory/")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("./directory/").mkdir()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.mkdir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.mkdir)
/// - [Python documentation: `os.mkdir`](https://docs.python.org/3/library/os.html#os.mkdir)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsMkdir;

impl Violation for OsMkdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.mkdir()` should be replaced by `Path.mkdir()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.stat`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.stat()` can improve readability over the `os`
/// module's counterparts (e.g., `os.path.stat()`).
///
/// ## Examples
/// ```python
/// import os
/// from pwd import getpwuid
/// from grp import getgrgid
///
/// stat = os.stat(file_name)
/// owner_name = getpwuid(stat.st_uid).pw_name
/// group_name = getgrgid(stat.st_gid).gr_name
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// file_path = Path(file_name)
/// stat = file_path.stat()
/// owner_name = file_path.owner()
/// group_name = file_path.group()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.stat`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.stat)
/// - [Python documentation: `Path.group`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.group)
/// - [Python documentation: `Path.owner`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.owner)
/// - [Python documentation: `os.stat`](https://docs.python.org/3/library/os.html#os.stat)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsStat;

impl Violation for OsStat {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.stat()` should be replaced by `Path.stat()`, `Path.owner()`, or `Path.group()`"
            .to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.path.join`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.joinpath()` or the `/` operator can improve
/// readability over the `os.path` module's counterparts (e.g., `os.path.join()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.join(os.path.join(ROOT_PATH, "folder"), "file.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(ROOT_PATH) / "folder" / "file.py"
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `PurePath.joinpath`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.joinpath)
/// - [Python documentation: `os.path.join`](https://docs.python.org/3/library/os.path.html#os.path.join)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathJoin {
    pub(crate) module: String,
    pub(crate) joiner: Joiner,
}

impl Violation for OsPathJoin {
    #[derive_message_formats]
    fn message(&self) -> String {
        let OsPathJoin { module, joiner } = self;
        match joiner {
            Joiner::Slash => {
                format!("`os.{module}.join()` should be replaced by `Path` with `/` operator")
            }
            Joiner::Joinpath => {
                format!("`os.{module}.join()` should be replaced by `Path.joinpath()`")
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Joiner {
    Slash,
    Joinpath,
}

/// ## What it does
/// Checks for uses of `os.path.splitext`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.suffix` and `Path.stem` can improve readability over
/// the `os.path` module's counterparts (e.g., `os.path.splitext()`).
///
/// `os.path.splitext()` specifically returns a tuple of the file root and
/// extension (e.g., given `splitext('/foo/bar.py')`, `os.path.splitext()`
/// returns `("foo/bar", ".py")`. These outputs can be reconstructed through a
/// combination of `Path.suffix` (`".py"`), `Path.stem` (`"bar"`), and
/// `Path.parent` (`"foo"`).
///
/// ## Examples
/// ```python
/// import os
///
/// (root, ext) = os.path.splitext("foo/bar.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// path = Path("foo/bar.py")
/// root = path.parent / path.stem
/// ext = path.suffix
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.suffix`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.suffix)
/// - [Python documentation: `Path.suffixes`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.suffixes)
/// - [Python documentation: `os.path.splitext`](https://docs.python.org/3/library/os.path.html#os.path.splitext)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathSplitext;

impl Violation for OsPathSplitext {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.splitext()` should be replaced by `Path.suffix`, `Path.stem`, and `Path.parent`"
            .to_string()
    }
}

/// ## What it does
/// Checks for uses of the `open()` builtin.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation. When possible,
/// using `Path` object methods such as `Path.open()` can improve readability
/// over the `open` builtin.
///
/// ## Examples
/// ```python
/// with open("f1.py", "wb") as fp:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// with Path("f1.py").open("wb") as fp:
///     ...
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than working directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.open`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.open)
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct BuiltinOpen;

impl Violation for BuiltinOpen {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`open()` should be replaced by `Path.open()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of the `py.path` library.
///
/// ## Why is this bad?
/// The `py.path` library is in maintenance mode. Instead, prefer the standard
/// library's `pathlib` module, or third-party modules like `path` (formerly
/// `py.path`).
///
/// ## Examples
/// ```python
/// import py.path
///
/// p = py.path.local("/foo/bar").join("baz/qux")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// p = Path("/foo/bar") / "bar" / "qux"
/// ```
///
/// ## References
/// - [Python documentation: `Pathlib`](https://docs.python.org/3/library/pathlib.html)
/// - [Path repository](https://github.com/jaraco/path)
#[derive(ViolationMetadata)]
pub(crate) struct PyPath;

impl Violation for PyPath {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`py.path` is in maintenance mode, use `pathlib` instead".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.listdir`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `pathlib`'s
/// `Path.iterdir()` can improve readability over `os.listdir()`.
///
/// ## Example
///
/// ```python
/// p = "."
/// for d in os.listdir(p):
///     ...
///
/// if os.listdir(p):
///     ...
///
/// if "file" in os.listdir(p):
///     ...
/// ```
///
/// Use instead:
///
/// ```python
/// p = Path(".")
/// for d in p.iterdir():
///     ...
///
/// if any(p.iterdir()):
///     ...
///
/// if (p / "file").exists():
///     ...
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.iterdir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.iterdir)
/// - [Python documentation: `os.listdir`](https://docs.python.org/3/library/os.html#os.listdir)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsListdir;

impl Violation for OsListdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `pathlib.Path.iterdir()` instead.".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.symlink`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.symlink`.
///
/// ## Example
/// ```python
/// import os
///
/// os.symlink("usr/bin/python", "tmp/python", target_is_directory=False)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("tmp/python").symlink_to("usr/bin/python")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.symlink_to`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.symlink_to)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsSymlink;

impl Violation for OsSymlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.symlink` should be replaced by `Path.symlink_to`".to_string()
    }
}
