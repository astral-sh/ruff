use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};

/// ## What it does
/// Checks for uses of `os.path.abspath`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.resolve()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.abspath()`).
///
/// ## Examples
/// ```python
/// import os
///
/// file_path = os.path.abspath("../path/to/file")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// file_path = Path("../path/to/file").resolve()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.resolve`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.resolve)
/// - [Python documentation: `os.path.abspath`](https://docs.python.org/3/library/os.path.html#os.path.abspath)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathAbspath;

impl Violation for OsPathAbspath {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.abspath()` should be replaced by `Path.resolve()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.chmod`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.chmod()` can improve readability over the `os`
/// module's counterparts (e.g., `os.chmod()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.chmod("file.py", 0o444)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("file.py").chmod(0o444)
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.chmod`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.chmod)
/// - [Python documentation: `os.chmod`](https://docs.python.org/3/library/os.html#os.chmod)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsChmod;

impl Violation for OsChmod {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.chmod()` should be replaced by `Path.chmod()`".to_string()
    }
}

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
/// Checks for uses of `os.rename`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.rename()` can improve readability over the `os`
/// module's counterparts (e.g., `os.rename()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.rename("old.py", "new.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("old.py").rename("new.py")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.rename`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.rename)
/// - [Python documentation: `os.rename`](https://docs.python.org/3/library/os.html#os.rename)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsRename;

impl Violation for OsRename {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.rename()` should be replaced by `Path.rename()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.replace`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.replace()` can improve readability over the `os`
/// module's counterparts (e.g., `os.replace()`).
///
/// Note that `os` functions may be preferable if performance is a concern,
/// e.g., in hot loops.
///
/// ## Examples
/// ```python
/// import os
///
/// os.replace("old.py", "new.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("old.py").replace("new.py")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.replace`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.replace)
/// - [Python documentation: `os.replace`](https://docs.python.org/3/library/os.html#os.replace)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsReplace;

impl Violation for OsReplace {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.replace()` should be replaced by `Path.replace()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.rmdir`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.rmdir()` can improve readability over the `os`
/// module's counterparts (e.g., `os.rmdir()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.rmdir("folder/")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("folder/").rmdir()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.rmdir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.rmdir)
/// - [Python documentation: `os.rmdir`](https://docs.python.org/3/library/os.html#os.rmdir)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsRmdir;

impl Violation for OsRmdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.rmdir()` should be replaced by `Path.rmdir()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.remove`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.unlink()` can improve readability over the `os`
/// module's counterparts (e.g., `os.remove()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.remove("file.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("file.py").unlink()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.unlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.unlink)
/// - [Python documentation: `os.remove`](https://docs.python.org/3/library/os.html#os.remove)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsRemove;

impl Violation for OsRemove {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.remove()` should be replaced by `Path.unlink()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.unlink`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.unlink()` can improve readability over the `os`
/// module's counterparts (e.g., `os.unlink()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.unlink("file.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("file.py").unlink()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.unlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.unlink)
/// - [Python documentation: `os.unlink`](https://docs.python.org/3/library/os.html#os.unlink)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsUnlink;

impl Violation for OsUnlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.unlink()` should be replaced by `Path.unlink()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.getcwd` and `os.getcwdb`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.cwd()` can improve readability over the `os`
/// module's counterparts (e.g., `os.getcwd()`).
///
/// ## Examples
/// ```python
/// import os
///
/// cwd = os.getcwd()
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// cwd = Path.cwd()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.cwd`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.cwd)
/// - [Python documentation: `os.getcwd`](https://docs.python.org/3/library/os.html#os.getcwd)
/// - [Python documentation: `os.getcwdb`](https://docs.python.org/3/library/os.html#os.getcwdb)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsGetcwd;

impl Violation for OsGetcwd {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.getcwd()` should be replaced by `Path.cwd()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.path.exists`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.exists()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.exists()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.exists("file.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("file.py").exists()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.exists`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.exists)
/// - [Python documentation: `os.path.exists`](https://docs.python.org/3/library/os.path.html#os.path.exists)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathExists;

impl Violation for OsPathExists {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.exists()` should be replaced by `Path.exists()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.path.expanduser`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.expanduser()` can improve readability over the `os.path`
/// module's counterparts (e.g., as `os.path.expanduser()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.expanduser("~/films/Monty Python")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("~/films/Monty Python").expanduser()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.expanduser`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.expanduser)
/// - [Python documentation: `os.path.expanduser`](https://docs.python.org/3/library/os.path.html#os.path.expanduser)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathExpanduser;

impl Violation for OsPathExpanduser {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.expanduser()` should be replaced by `Path.expanduser()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.path.isdir`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.is_dir()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.isdir()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.isdir("docs")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("docs").is_dir()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.is_dir`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.is_dir)
/// - [Python documentation: `os.path.isdir`](https://docs.python.org/3/library/os.path.html#os.path.isdir)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathIsdir;

impl Violation for OsPathIsdir {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.isdir()` should be replaced by `Path.is_dir()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.path.isfile`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.is_file()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.isfile()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.isfile("docs")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("docs").is_file()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.is_file`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.is_file)
/// - [Python documentation: `os.path.isfile`](https://docs.python.org/3/library/os.path.html#os.path.isfile)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathIsfile;

impl Violation for OsPathIsfile {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.isfile()` should be replaced by `Path.is_file()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.path.islink`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.is_symlink()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.islink()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.islink("docs")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("docs").is_symlink()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.is_symlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.is_symlink)
/// - [Python documentation: `os.path.islink`](https://docs.python.org/3/library/os.path.html#os.path.islink)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathIslink;

impl Violation for OsPathIslink {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.islink()` should be replaced by `Path.is_symlink()`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.readlink`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os`. When possible, using `Path` object
/// methods such as `Path.readlink()` can improve readability over the `os`
/// module's counterparts (e.g., `os.readlink()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.readlink(file_name)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(file_name).readlink()
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.readlink`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.readline)
/// - [Python documentation: `os.readlink`](https://docs.python.org/3/library/os.html#os.readlink)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsReadlink;

impl Violation for OsReadlink {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.readlink()` should be replaced by `Path.readlink()`".to_string()
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
/// Checks for uses of `os.path.isabs`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.is_absolute()` can improve readability over the `os.path`
/// module's counterparts (e.g.,  as `os.path.isabs()`).
///
/// ## Examples
/// ```python
/// import os
///
/// if os.path.isabs(file_name):
///     print("Absolute path!")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// if Path(file_name).is_absolute():
///     print("Absolute path!")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `PurePath.is_absolute`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.is_absolute)
/// - [Python documentation: `os.path.isabs`](https://docs.python.org/3/library/os.path.html#os.path.isabs)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathIsabs;

impl Violation for OsPathIsabs {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.isabs()` should be replaced by `Path.is_absolute()`".to_string()
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
/// Checks for uses of `os.path.basename`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.name` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.basename()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.basename(__file__)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(__file__).name
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `PurePath.name`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.name)
/// - [Python documentation: `os.path.basename`](https://docs.python.org/3/library/os.path.html#os.path.basename)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathBasename;

impl Violation for OsPathBasename {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.basename()` should be replaced by `Path.name`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.path.dirname`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.parent` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.dirname()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.dirname(__file__)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(__file__).parent
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `PurePath.parent`](https://docs.python.org/3/library/pathlib.html#pathlib.PurePath.parent)
/// - [Python documentation: `os.path.dirname`](https://docs.python.org/3/library/os.path.html#os.path.dirname)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathDirname;

impl Violation for OsPathDirname {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.dirname()` should be replaced by `Path.parent`".to_string()
    }
}

/// ## What it does
/// Checks for uses of `os.path.samefile`.
///
/// ## Why is this bad?
/// `pathlib` offers a high-level API for path manipulation, as compared to
/// the lower-level API offered by `os.path`. When possible, using `Path` object
/// methods such as `Path.samefile()` can improve readability over the `os.path`
/// module's counterparts (e.g., `os.path.samefile()`).
///
/// ## Examples
/// ```python
/// import os
///
/// os.path.samefile("f1.py", "f2.py")
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path("f1.py").samefile("f2.py")
/// ```
///
/// ## Known issues
/// While using `pathlib` can improve the readability and type safety of your code,
/// it can be less performant than the lower-level alternatives that work directly with strings,
/// especially on older versions of Python.
///
/// ## References
/// - [Python documentation: `Path.samefile`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.samefile)
/// - [Python documentation: `os.path.samefile`](https://docs.python.org/3/library/os.path.html#os.path.samefile)
/// - [PEP 428 – The pathlib module – object-oriented filesystem paths](https://peps.python.org/pep-0428/)
/// - [Correspondence between `os` and `pathlib`](https://docs.python.org/3/library/pathlib.html#correspondence-to-tools-in-the-os-module)
/// - [Why you should be using pathlib](https://treyhunner.com/2018/12/why-you-should-be-using-pathlib/)
/// - [No really, pathlib is great](https://treyhunner.com/2019/01/no-really-pathlib-is-great/)
#[derive(ViolationMetadata)]
pub(crate) struct OsPathSamefile;

impl Violation for OsPathSamefile {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`os.path.samefile()` should be replaced by `Path.samefile()`".to_string()
    }
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
