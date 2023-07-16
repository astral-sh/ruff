use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};

// PTH100

/// ## What it does
/// Detects the use of `os.path.abspath`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.resolve()` improves readability over their `os` counterparts such as
/// `os.path.abspath()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// file_path = os.path.abspath("../path/to/file")
/// ```
///
/// Use instead:
/// ```python
/// file_path = Path("../path/to/file").resolve()
/// ```
///
/// ## References
/// - [Python documentation: `Path.resolve`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.resolve)
/// - [Python documentation: `os.path.abspath`](https://docs.python.org/3/library/os.path.html#os.path.abspath)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathAbspath;

impl Violation for OsPathAbspath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.abspath()` should be replaced by `Path.resolve()`")
    }
}

// PTH101

/// ## What it does
/// Detects the use of `os.chmod`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.chmod()` improves readability over their `os` counterparts such as
/// `os.chmod()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.chmod("file.py", 0o444)
/// ```
///
/// Use instead:
/// ```python
/// Path("file.py").chmod(0o444)
/// ```
///
/// ## References
/// - [Python documentation: `Path.chmod`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.chmod)
/// - [Python documentation: `os.chmod`](https://docs.python.org/3/library/os.html#os.chmod)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsChmod;

impl Violation for OsChmod {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.chmod()` should be replaced by `Path.chmod()`")
    }
}

// PTH102

/// ## What it does
/// Detects the use of `os.makedirs`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.mkdir(parents=True)` improves readability over their `os`
/// counterparts such as `os.makedirs()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.makedirs("./nested/directory/")
/// ```
///
/// Use instead:
/// ```python
/// Path("./nested/directory/").mkdir(parents=True)
/// ```
///
/// ## References
/// - [Python documentation: `Path.mkdir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.mkdir)
/// - [Python documentation: `os.makedirs`](https://docs.python.org/3/library/os.html#os.makedirs)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsMakedirs;

impl Violation for OsMakedirs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.makedirs()` should be replaced by `Path.mkdir(parents=True)`")
    }
}

// PTH103

/// ## What it does
/// Detects the use of `os.mkdir`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.mkdir()` improves readability over their `os` counterparts such as
/// `os.mkdir()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.mkdir("./directory/")
/// ```
///
/// Use instead:
/// ```python
/// Path("./directory/").mkdir()
/// ```
///
/// ## References
/// - [Python documentation: `Path.mkdir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.mkdir)
/// - [Python documentation: `os.mkdir`](https://docs.python.org/3/library/os.html#os.mkdir)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsMkdir;

impl Violation for OsMkdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.mkdir()` should be replaced by `Path.mkdir()`")
    }
}

// PTH104

/// ## What it does
/// Detects the use of `os.rename`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.rename()` improves readability over their `os` counterparts such as
/// `os.rename()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.rename("old.py", "new.py")
/// ```
///
/// Use instead:
/// ```python
/// Path("old.py").rename("new.py")
/// ```
///
/// ## References
/// - [Python documentation: `Path.rename`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.rename)
/// - [Python documentation: `os.rename`](https://docs.python.org/3/library/os.html#os.rename)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsRename;

impl Violation for OsRename {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.rename()` should be replaced by `Path.rename()`")
    }
}

// PTH105

/// ## What it does
/// Detects the use of `os.replace`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.replace()` improves readability over their `os` counterparts such as
/// `os.replace()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.replace("old.py", "new.py")
/// ```
///
/// Use instead:
/// ```python
/// Path("old.py").replace("new.py")
/// ```
///
/// ## References
/// - [Python documentation: `Path.replace`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.replace)
/// - [Python documentation: `os.replace`](https://docs.python.org/3/library/os.html#os.replace)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsReplace;

impl Violation for OsReplace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.replace()` should be replaced by `Path.replace()`")
    }
}

// PTH106

/// ## What it does
/// Detects the use of `os.rmdir`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.rmdir()` improves readability over their `os` counterparts such as
/// `os.rmdir()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.rmdir("folder/")
/// ```
///
/// Use instead:
/// ```python
/// Path("folder/").rmdir()
/// ```
///
/// ## References
/// - [Python documentation: `Path.rmdir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.rmdir)
/// - [Python documentation: `os.rmdir`](https://docs.python.org/3/library/os.html#os.rmdir)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsRmdir;

impl Violation for OsRmdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.rmdir()` should be replaced by `Path.rmdir()`")
    }
}

// PTH107

/// ## What it does
/// Detects the use of `os.remove`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.unlink()` improves readability over their `os` counterparts such as
/// `os.remove()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.remove("file.py")
/// ```
///
/// Use instead:
/// ```python
/// Path("file.py").unlink()
/// ```
///
/// ## References
/// - [Python documentation: `Path.unlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.unlink)
/// - [Python documentation: `os.remove`](https://docs.python.org/3/library/os.html#os.remove)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsRemove;

impl Violation for OsRemove {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.remove()` should be replaced by `Path.unlink()`")
    }
}

// PTH108

/// ## What it does
/// Detects the use of `os.unlink`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.unlink()` improves readability over their `os` counterparts such as
/// `os.unlink()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.unlink("file.py")
/// ```
///
/// Use instead:
/// ```python
/// Path("file.py").unlink()
/// ```
///
/// ## References
/// - [Python documentation: `Path.unlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.unlink)
/// - [Python documentation: `os.unlink`](https://docs.python.org/3/library/os.html#os.unlink)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsUnlink;

impl Violation for OsUnlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.unlink()` should be replaced by `Path.unlink()`")
    }
}

// PTH109

/// ## What it does
/// Detects the use of `os.getcwd` and `os.getcwdb`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.cwd()` improves readability over their `os` counterparts such as
/// `os.getcwd()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// cwd = os.getcwd()
/// ```
///
/// Use instead:
/// ```python
/// cwd = Path.cwd()
/// ```
///
/// ## References
/// - [Python documentation: `Path.cwd`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.cwd)
/// - [Python documentation: `os.getcwd`](https://docs.python.org/3/library/os.html#os.getcwd)
/// - [Python documentation: `os.getcwdb`](https://docs.python.org/3/library/os.html#os.getcwdb)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsGetcwd;

impl Violation for OsGetcwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.getcwd()` should be replaced by `Path.cwd()`")
    }
}

// PTH110

/// ## What it does
/// Detects the use of `os.path.exists`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.exists()` improves readability over their `os` counterparts such as
/// `os.path.exists()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.exists("file.py")
/// ```
///
/// Use instead:
/// ```python
/// Path("file.py").exists()
/// ```
///
/// ## References
/// - [Python documentation: `Path.exists`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.exists)
/// - [Python documentation: `os.path.exists`](https://docs.python.org/3/library/os.path.html#os.path.exists)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathExists;

impl Violation for OsPathExists {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.exists()` should be replaced by `Path.exists()`")
    }
}

// PTH111

/// ## What it does
/// Detects the use of `os.path.expanduser`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.expanduser()` improves readability over their `os` counterparts such
/// as `os.path.expanduser()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.expanduser("~/films/Monty Python")
/// ```
///
/// Use instead:
/// ```python
/// Path("~/films/Monty Python").expanduser()
/// ```
///
/// ## References
/// - [Python documentation: `Path.expanduser`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.expanduser)
/// - [Python documentation: `os.path.expanduser`](https://docs.python.org/3/library/os.path.html#os.path.expanduser)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathExpanduser;

impl Violation for OsPathExpanduser {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.expanduser()` should be replaced by `Path.expanduser()`")
    }
}

// PTH112

/// ## What it does
/// Detects the use of `os.path.isdir`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.is_dir()` improves readability over their `os` counterparts such as
/// `os.path.isdir()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.isdir("docs")
/// ```
///
/// Use instead:
/// ```python
/// Path("docs").is_dir()
/// ```
///
/// ## References
/// - [Python documentation: `Path.is_dir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.is_dir)
/// - [Python documentation: `os.path.isdir`](https://docs.python.org/3/library/os.path.html#os.path.isdir)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathIsdir;

impl Violation for OsPathIsdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isdir()` should be replaced by `Path.is_dir()`")
    }
}

// PTH113

/// ## What it does
/// Detects the use of `os.path.isfile`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.is_file()` improves readability over their `os` counterparts such as
/// `os.path.isfile()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.isfile("docs")
/// ```
///
/// Use instead:
/// ```python
/// Path("docs").is_file()
/// ```
///
/// ## References
/// - [Python documentation: `Path.is_file`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.is_file)
/// - [Python documentation: `os.path.isfile`](https://docs.python.org/3/library/os.path.html#os.path.isfile)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathIsfile;

impl Violation for OsPathIsfile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isfile()` should be replaced by `Path.is_file()`")
    }
}

// PTH114

/// ## What it does
/// Detects the use of `os.path.islink`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.is_link()` improves readability over their `os` counterparts such as
/// `os.path.islink()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.islink("docs")
/// ```
///
/// Use instead:
/// ```python
/// Path("docs").is_link()
/// ```
///
/// ## References
/// - [Python documentation: `Path.is_link`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.is_link)
/// - [Python documentation: `os.path.islink`](https://docs.python.org/3/library/os.path.html#os.path.islink)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathIslink;

impl Violation for OsPathIslink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.islink()` should be replaced by `Path.is_symlink()`")
    }
}

// PTH115

/// ## What it does
/// Detects the use of `os.readlink`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.readlink()` improves readability over their `os` counterparts such as
/// `os.readlink()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.readlink(file_name)
/// ```
///
/// Use instead:
/// ```python
/// Path(file_name).readlink()
/// ```
///
/// ## References
/// - [Python documentation: `Path.readlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.readline)
/// - [Python documentation: `os.readlink`](https://docs.python.org/3/library/os.html#os.readlink)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsReadlink;

impl Violation for OsReadlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.readlink()` should be replaced by `Path.readlink()`")
    }
}

// PTH116

/// ## What it does
/// Detects the use of `os.stat`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.stat()` improves readability over their `os` counterparts such as
/// `os.path.stat()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
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
/// ## References
/// - [Python documentation: `Path.stat`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.group)
/// - [Python documentation: `Path.group`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.group)
/// - [Python documentation: `Path.owner`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.owner)
/// - [Python documentation: `os.stat`](https://docs.python.org/3/library/os.html#os.stat)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
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

/// ## What it does
/// Detects the use of `os.path.isabs`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.is_absolute()` improves readability over their `os` counterparts such
///  as `os.path.isabs()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// if os.path.isabs(file_name):
///     print("Absolute path!")
/// ```
///
/// Use instead:
/// ```python
/// if Path(file_name).is_absolute():
///     print("Absolute path!")
/// ```
///
/// ## References
/// - [Python documentation: `PurePath.is_absolute`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.is_absolute)
/// - [Python documentation: `os.path.isabs`](https://docs.python.org/3/library/os.path.html#os.path.isabs)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathIsabs;

impl Violation for OsPathIsabs {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.isabs()` should be replaced by `Path.is_absolute()`")
    }
}

// PTH118

/// ## What it does
/// Detects the use of `os.path.join`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.joinpath()` improves readability over their `os` counterparts such as
/// `os.path.join()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.join(os.path.join(ROOT_PATH, "folder"), "file.py")
/// ```
///
/// Use instead:
/// ```python
/// Path(ROOT_PATH) / "folder" / "file.py"
/// ```
///
/// ## References
/// - [Python documentation: `PurePath.joinpath`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.joinpath)
/// - [Python documentation: `os.path.join`](https://docs.python.org/3/library/os.path.html#os.path.join)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathJoin;

impl Violation for OsPathJoin {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.join()` should be replaced by `Path` with `/` operator")
    }
}

// PTH119

/// ## What it does
/// Detects the use of `os.path.basename`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.name` improves readability over their `os` counterparts such as
/// `os.path.basename()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.basename(__file__)
/// ```
///
/// Use instead:
/// ```python
/// Path(__file__).name
/// ```
///
/// ## References
/// - [Python documentation: `PurePath.name`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.name)
/// - [Python documentation: `os.path.basename`](https://docs.python.org/3/library/os.path.html#os.path.basename)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathBasename;

impl Violation for OsPathBasename {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.basename()` should be replaced by `Path.name`")
    }
}

// PTH120

/// ## What it does
/// Detects the use of `os.path.dirname`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.parent` improves readability over their `os` counterparts such as
/// `os.path.dirname()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.dirname(__file__)
/// ```
///
/// Use instead:
/// ```python
/// Path(__file__).parent
/// ```
///
/// ## References
/// - [Python documentation: `PurePath.parent`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.parent)
/// - [Python documentation: `os.path.dirname`](https://docs.python.org/3/library/os.path.html#os.path.dirname)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathDirname;

impl Violation for OsPathDirname {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.dirname()` should be replaced by `Path.parent`")
    }
}

// PTH121

/// ## What it does
/// Detects the use of `os.path.samefile`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.samefile()` improves readability over their `os` counterparts such as
/// `os.path.samefile()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.samefile("f1.py", "f2.py")
/// ```
///
/// Use instead:
/// ```python
/// Path("f1.py").samefile("f2.py")
/// ```
///
/// ## References
/// - [Python documentation: `Path.samefile`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.samefile)
/// - [Python documentation: `os.path.samefile`](https://docs.python.org/3/library/os.path.html#os.path.samefile)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathSamefile;

impl Violation for OsPathSamefile {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.samefile()` should be replaced by `Path.samefile()`")
    }
}

// PTH122

/// ## What it does
/// Detects the use of `os.path.splitext`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.suffix` improves readability over their `os` counterparts such as
/// `os.path.splitext()`.
///
/// There are situations where creating many `Path` object causes overhead.
/// `os` functions therefore remain preferable in heavy loops and data
/// structures storing paths (e.g. pandas).
///
/// ## Examples
/// ```python
/// os.path.splitext("f1.py")
/// ```
///
/// Use instead:
/// ```python
/// Path("f1.py").suffix
/// ```
///
/// ## References
/// - [Python documentation: `Path.suffix`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.suffix)
/// - [Python documentation: `Path.suffixes`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.suffixes)
/// - [Python documentation: `os.path.splitext`](https://docs.python.org/3/library/os.path.html#os.path.splitext)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct OsPathSplitext;

impl Violation for OsPathSplitext {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`os.path.splitext()` should be replaced by `Path.suffix`")
    }
}

// PTH123

/// ## What it does
/// Detects the use of `open`.
///
/// ## Why is this bad?
/// `pathlib` offers high-level path manipulations, `os` offers low-level
/// manipulation of paths. The use of the `Path` object where possible, e.g.
/// `Path.open()` improves readability over their `os` counterparts that are
/// often used to manipulate the input argument to `open()`.
///
///
/// ## Examples
/// ```python
/// with open("f1.py", "wb") as fp:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// with Path("f1.py").open("wb") as fp:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: `Path.open`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.open)
/// - [Python documentation: `open`](https://docs.python.org/3/library/functions.html#open)
/// - [PEP 428](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[violation]
pub struct BuiltinOpen;

impl Violation for BuiltinOpen {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`open()` should be replaced by `Path.open()`")
    }
}

// PTH124

/// ## What it does
/// Detects the use of the `py.path` lirbary.
///
/// ## Why is this bad?
/// `py.path` is in maintenance mode, use the `pathlib` module from the
/// standard library, or third-party modules such as `path` (formerly
/// `py.path`).
///
/// ## Examples
/// ```python
/// p = py.path.local("/foo/bar").join("baz/qux")
/// ```
///
/// Use instead:
/// ```python
/// p = Path("/foo/bar") / "bar" / "qux"
/// ```
///
/// ## References
/// - [Python documentation: `Pathlib`](https://docs.python.org/3/library/pathlib.html)
/// - [Path repository](https://github.com/jaraco/path)
#[violation]
pub struct PyPath;

impl Violation for PyPath {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`py.path` is in maintenance mode, use `pathlib` instead")
    }
}
