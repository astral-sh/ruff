/// A list of all builtins that are available in IPython.
///
/// How to create this list:
/// ```python
/// import json
/// from subprocess import check_output
///
/// builtins_python = json.loads(check_output(["python3", "-c" "import json; print(json.dumps(dir(__builtins__)))"]))
/// builtins_ipython = json.loads(check_output(["ipython3", "-c" "import json; print(json.dumps(dir(__builtins__)))"]))
/// print(sorted(set(builtins_ipython) - set(builtins_python)))
/// ```
///
/// Intended to be kept in sync with [`is_ipython_builtin`].
const IPYTHON_BUILTINS: &[&str] = &["__IPYTHON__", "display", "get_ipython"];

/// Globally defined names which are not attributes of the builtins module, or
/// are only present on some platforms.
pub const MAGIC_GLOBALS: &[&str] = &[
    "WindowsError",
    "__annotations__",
    "__builtins__",
    "__cached__",
    "__warningregistry__",
    "__file__",
];

/// Magic globals that are only available starting in specific Python versions.
///
/// `__annotate__` was introduced in Python 3.14.
static PY314_PLUS_MAGIC_GLOBALS: &[&str] = &["__annotate__"];

static ALWAYS_AVAILABLE_BUILTINS: &[&str] = &[
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "BlockingIOError",
    "BrokenPipeError",
    "BufferError",
    "BytesWarning",
    "ChildProcessError",
    "ConnectionAbortedError",
    "ConnectionError",
    "ConnectionRefusedError",
    "ConnectionResetError",
    "DeprecationWarning",
    "EOFError",
    "Ellipsis",
    "EnvironmentError",
    "Exception",
    "False",
    "FileExistsError",
    "FileNotFoundError",
    "FloatingPointError",
    "FutureWarning",
    "GeneratorExit",
    "IOError",
    "ImportError",
    "ImportWarning",
    "IndentationError",
    "IndexError",
    "InterruptedError",
    "IsADirectoryError",
    "KeyError",
    "KeyboardInterrupt",
    "LookupError",
    "MemoryError",
    "ModuleNotFoundError",
    "NameError",
    "None",
    "NotADirectoryError",
    "NotImplemented",
    "NotImplementedError",
    "OSError",
    "OverflowError",
    "PendingDeprecationWarning",
    "PermissionError",
    "ProcessLookupError",
    "RecursionError",
    "ReferenceError",
    "ResourceWarning",
    "RuntimeError",
    "RuntimeWarning",
    "StopAsyncIteration",
    "StopIteration",
    "SyntaxError",
    "SyntaxWarning",
    "SystemError",
    "SystemExit",
    "TabError",
    "TimeoutError",
    "True",
    "TypeError",
    "UnboundLocalError",
    "UnicodeDecodeError",
    "UnicodeEncodeError",
    "UnicodeError",
    "UnicodeTranslateError",
    "UnicodeWarning",
    "UserWarning",
    "ValueError",
    "Warning",
    "ZeroDivisionError",
    "__build_class__",
    "__debug__",
    "__doc__",
    "__import__",
    "__loader__",
    "__name__",
    "__package__",
    "__spec__",
    "abs",
    "all",
    "any",
    "ascii",
    "bin",
    "bool",
    "breakpoint",
    "bytearray",
    "bytes",
    "callable",
    "chr",
    "classmethod",
    "compile",
    "complex",
    "copyright",
    "credits",
    "delattr",
    "dict",
    "dir",
    "divmod",
    "enumerate",
    "eval",
    "exec",
    "exit",
    "filter",
    "float",
    "format",
    "frozenset",
    "getattr",
    "globals",
    "hasattr",
    "hash",
    "help",
    "hex",
    "id",
    "input",
    "int",
    "isinstance",
    "issubclass",
    "iter",
    "len",
    "license",
    "list",
    "locals",
    "map",
    "max",
    "memoryview",
    "min",
    "next",
    "object",
    "oct",
    "open",
    "ord",
    "pow",
    "print",
    "property",
    "quit",
    "range",
    "repr",
    "reversed",
    "round",
    "set",
    "setattr",
    "slice",
    "sorted",
    "staticmethod",
    "str",
    "sum",
    "super",
    "tuple",
    "type",
    "vars",
    "zip",
];
static PY310_PLUS_BUILTINS: &[&str] = &["EncodingWarning", "aiter", "anext"];
static PY311_PLUS_BUILTINS: &[&str] = &["BaseExceptionGroup", "ExceptionGroup"];
static PY313_PLUS_BUILTINS: &[&str] = &["PythonFinalizationError"];
static PY315_PLUS_BUILTINS: &[&str] = &["frozendict"];

/// Return the list of builtins for the given Python minor version.
///
/// Intended to be kept in sync with [`is_python_builtin`].
pub fn python_builtins(minor_version: u8, is_notebook: bool) -> impl Iterator<Item = &'static str> {
    let py310_builtins = if minor_version >= 10 {
        Some(PY310_PLUS_BUILTINS)
    } else {
        None
    };
    let py311_builtins = if minor_version >= 11 {
        Some(PY311_PLUS_BUILTINS)
    } else {
        None
    };
    let py313_builtins = if minor_version >= 13 {
        Some(PY313_PLUS_BUILTINS)
    } else {
        None
    };
    let py315_builtins = if minor_version >= 15 {
        Some(PY315_PLUS_BUILTINS)
    } else {
        None
    };
    let ipython_builtins = if is_notebook {
        Some(IPYTHON_BUILTINS)
    } else {
        None
    };

    py310_builtins
        .into_iter()
        .chain(py311_builtins)
        .chain(py313_builtins)
        .chain(py315_builtins)
        .chain(ipython_builtins)
        .flatten()
        .chain(ALWAYS_AVAILABLE_BUILTINS)
        .copied()
}

/// Return the list of magic globals for the given Python minor version.
pub fn python_magic_globals(minor_version: u8) -> impl Iterator<Item = &'static str> {
    let py314_magic_globals = if minor_version >= 14 {
        Some(PY314_PLUS_MAGIC_GLOBALS)
    } else {
        None
    };

    py314_magic_globals
        .into_iter()
        .flatten()
        .chain(MAGIC_GLOBALS)
        .copied()
}

/// Returns `true` if the given name is that of a Python builtin.
///
/// Intended to be kept in sync with [`python_builtins`].
pub fn is_python_builtin(name: &str, minor_version: u8, is_notebook: bool) -> bool {
    if is_notebook && is_ipython_builtin(name) {
        return true;
    }
    matches!(
        (minor_version, name),
        (
            _,
            "ArithmeticError"
                | "AssertionError"
                | "AttributeError"
                | "BaseException"
                | "BlockingIOError"
                | "BrokenPipeError"
                | "BufferError"
                | "BytesWarning"
                | "ChildProcessError"
                | "ConnectionAbortedError"
                | "ConnectionError"
                | "ConnectionRefusedError"
                | "ConnectionResetError"
                | "DeprecationWarning"
                | "EOFError"
                | "Ellipsis"
                | "EnvironmentError"
                | "Exception"
                | "False"
                | "FileExistsError"
                | "FileNotFoundError"
                | "FloatingPointError"
                | "FutureWarning"
                | "GeneratorExit"
                | "IOError"
                | "ImportError"
                | "ImportWarning"
                | "IndentationError"
                | "IndexError"
                | "InterruptedError"
                | "IsADirectoryError"
                | "KeyError"
                | "KeyboardInterrupt"
                | "LookupError"
                | "MemoryError"
                | "ModuleNotFoundError"
                | "NameError"
                | "None"
                | "NotADirectoryError"
                | "NotImplemented"
                | "NotImplementedError"
                | "OSError"
                | "OverflowError"
                | "PendingDeprecationWarning"
                | "PermissionError"
                | "ProcessLookupError"
                | "RecursionError"
                | "ReferenceError"
                | "ResourceWarning"
                | "RuntimeError"
                | "RuntimeWarning"
                | "StopAsyncIteration"
                | "StopIteration"
                | "SyntaxError"
                | "SyntaxWarning"
                | "SystemError"
                | "SystemExit"
                | "TabError"
                | "TimeoutError"
                | "True"
                | "TypeError"
                | "UnboundLocalError"
                | "UnicodeDecodeError"
                | "UnicodeEncodeError"
                | "UnicodeError"
                | "UnicodeTranslateError"
                | "UnicodeWarning"
                | "UserWarning"
                | "ValueError"
                | "Warning"
                | "ZeroDivisionError"
                | "__build_class__"
                | "__debug__"
                | "__doc__"
                | "__import__"
                | "__loader__"
                | "__name__"
                | "__package__"
                | "__spec__"
                | "abs"
                | "all"
                | "any"
                | "ascii"
                | "bin"
                | "bool"
                | "breakpoint"
                | "bytearray"
                | "bytes"
                | "callable"
                | "chr"
                | "classmethod"
                | "compile"
                | "complex"
                | "copyright"
                | "credits"
                | "delattr"
                | "dict"
                | "dir"
                | "divmod"
                | "enumerate"
                | "eval"
                | "exec"
                | "exit"
                | "filter"
                | "float"
                | "format"
                | "frozenset"
                | "getattr"
                | "globals"
                | "hasattr"
                | "hash"
                | "help"
                | "hex"
                | "id"
                | "input"
                | "int"
                | "isinstance"
                | "issubclass"
                | "iter"
                | "len"
                | "license"
                | "list"
                | "locals"
                | "map"
                | "max"
                | "memoryview"
                | "min"
                | "next"
                | "object"
                | "oct"
                | "open"
                | "ord"
                | "pow"
                | "print"
                | "property"
                | "quit"
                | "range"
                | "repr"
                | "reversed"
                | "round"
                | "set"
                | "setattr"
                | "slice"
                | "sorted"
                | "staticmethod"
                | "str"
                | "sum"
                | "super"
                | "tuple"
                | "type"
                | "vars"
                | "zip"
        ) | (10.., "EncodingWarning" | "aiter" | "anext")
            | (11.., "BaseExceptionGroup" | "ExceptionGroup")
            | (13.., "PythonFinalizationError")
            | (15.., "frozendict")
    )
}

/// Return `Some(version)`, where `version` corresponds to the Python minor version
/// in which the builtin was added
pub fn version_builtin_was_added(name: &str) -> Option<u8> {
    if PY310_PLUS_BUILTINS.contains(&name) {
        Some(10)
    } else if PY311_PLUS_BUILTINS.contains(&name) {
        Some(11)
    } else if PY313_PLUS_BUILTINS.contains(&name) {
        Some(13)
    } else if PY315_PLUS_BUILTINS.contains(&name) {
        Some(15)
    } else if ALWAYS_AVAILABLE_BUILTINS.contains(&name) {
        Some(0)
    } else {
        None
    }
}

/// Returns `true` if the given name is that of a Python builtin iterator.
pub fn is_iterator(name: &str) -> bool {
    matches!(
        name,
        "enumerate" | "filter" | "map" | "reversed" | "zip" | "iter"
    )
}

/// Returns `true` if the given name is that of an IPython builtin.
///
/// Intended to be kept in sync with [`IPYTHON_BUILTINS`].
fn is_ipython_builtin(name: &str) -> bool {
    // Constructed by converting the `IPYTHON_BUILTINS` slice to a `match` expression.
    matches!(name, "__IPYTHON__" | "display" | "get_ipython")
}

/// Returns `true` if the given name is that of a builtin exception.
///
/// See: <https://docs.python.org/3/library/exceptions.html#exception-hierarchy>
pub fn is_exception(name: &str, minor_version: u8) -> bool {
    matches!(
        (minor_version, name),
        (
            _,
            "BaseException"
                | "GeneratorExit"
                | "KeyboardInterrupt"
                | "SystemExit"
                | "Exception"
                | "ArithmeticError"
                | "FloatingPointError"
                | "OverflowError"
                | "ZeroDivisionError"
                | "AssertionError"
                | "AttributeError"
                | "BufferError"
                | "EOFError"
                | "ImportError"
                | "ModuleNotFoundError"
                | "LookupError"
                | "IndexError"
                | "KeyError"
                | "MemoryError"
                | "NameError"
                | "UnboundLocalError"
                | "OSError"
                | "BlockingIOError"
                | "ChildProcessError"
                | "ConnectionError"
                | "BrokenPipeError"
                | "ConnectionAbortedError"
                | "ConnectionRefusedError"
                | "ConnectionResetError"
                | "FileExistsError"
                | "FileNotFoundError"
                | "InterruptedError"
                | "IsADirectoryError"
                | "NotADirectoryError"
                | "PermissionError"
                | "ProcessLookupError"
                | "TimeoutError"
                | "ReferenceError"
                | "RuntimeError"
                | "NotImplementedError"
                | "RecursionError"
                | "StopAsyncIteration"
                | "StopIteration"
                | "SyntaxError"
                | "IndentationError"
                | "TabError"
                | "SystemError"
                | "TypeError"
                | "ValueError"
                | "UnicodeError"
                | "UnicodeDecodeError"
                | "UnicodeEncodeError"
                | "UnicodeTranslateError"
                | "Warning"
                | "BytesWarning"
                | "DeprecationWarning"
                | "FutureWarning"
                | "ImportWarning"
                | "PendingDeprecationWarning"
                | "ResourceWarning"
                | "RuntimeWarning"
                | "SyntaxWarning"
                | "UnicodeWarning"
                | "UserWarning"
        ) | (10.., "EncodingWarning")
            | (11.., "BaseExceptionGroup" | "ExceptionGroup")
            | (13.., "PythonFinalizationError")
    )
}

/// Returns the direct superclass of a builtin exception in the Python exception hierarchy,
/// or `None` if the name is not a known builtin exception (or is `BaseException`, the root)
///
/// Also handles legacy aliases: `IOError` and `EnvironmentError` are aliases for `OSError`
///
/// See: <https://docs.python.org/3/library/exceptions.html#exception-hierarchy>
///
/// ```
/// use ruff_python_stdlib::builtins::builtin_exception_superclass;
///
/// assert_eq!(builtin_exception_superclass("ValueError"), Some("Exception"));
/// assert_eq!(builtin_exception_superclass("KeyError"), Some("LookupError"));
/// assert_eq!(builtin_exception_superclass("Exception"), Some("BaseException"));
/// assert_eq!(builtin_exception_superclass("BaseException"), None);
/// assert_eq!(builtin_exception_superclass("MyCustomError"), None);
///
/// // Aliases map to OSError
/// assert_eq!(builtin_exception_superclass("IOError"), Some("OSError"));
/// assert_eq!(builtin_exception_superclass("EnvironmentError"), Some("OSError"));
/// ```
pub fn builtin_exception_superclass(name: &str) -> Option<&'static str> {
    match name {
        // Direct children of BaseException
        "GeneratorExit" | "KeyboardInterrupt" | "SystemExit" | "Exception" => Some("BaseException"),
        // 3.11+: BaseExceptionGroup inherits from BaseException
        "BaseExceptionGroup" => Some("BaseException"),

        // Direct children of Exception
        "ArithmeticError" | "AssertionError" | "AttributeError" | "BufferError" | "EOFError"
        | "ImportError" | "LookupError" | "MemoryError" | "NameError" | "OSError"
        | "ReferenceError" | "RuntimeError" | "StopAsyncIteration" | "StopIteration"
        | "SyntaxError" | "SystemError" | "TypeError" | "ValueError" | "Warning" => {
            Some("Exception")
        }
        // 3.11+: ExceptionGroup inherits from Exception (and BaseExceptionGroup)
        "ExceptionGroup" => Some("Exception"),
        // 3.13+
        "PythonFinalizationError" => Some("Exception"),

        // ArithmeticError subclasses
        "FloatingPointError" | "OverflowError" | "ZeroDivisionError" => Some("ArithmeticError"),

        // ImportError subclasses
        "ModuleNotFoundError" => Some("ImportError"),

        // LookupError subclasses
        "IndexError" | "KeyError" => Some("LookupError"),

        // NameError subclasses
        "UnboundLocalError" => Some("NameError"),

        // OSError subclasses
        "BlockingIOError" | "ChildProcessError" | "ConnectionError" | "FileExistsError"
        | "FileNotFoundError" | "InterruptedError" | "IsADirectoryError" | "NotADirectoryError"
        | "PermissionError" | "ProcessLookupError" | "TimeoutError" => Some("OSError"),

        // OSError aliases
        "IOError" | "EnvironmentError" => Some("OSError"),

        // ConnectionError subclasses
        "BrokenPipeError"
        | "ConnectionAbortedError"
        | "ConnectionRefusedError"
        | "ConnectionResetError" => Some("ConnectionError"),

        // RuntimeError subclasses
        "NotImplementedError" | "RecursionError" => Some("RuntimeError"),

        // SyntaxError subclasses
        "IndentationError" => Some("SyntaxError"),

        // IndentationError subclasses
        "TabError" => Some("IndentationError"),

        // ValueError subclasses
        "UnicodeError" => Some("ValueError"),

        // UnicodeError subclasses
        "UnicodeDecodeError" | "UnicodeEncodeError" | "UnicodeTranslateError" => {
            Some("UnicodeError")
        }

        // Warning subclasses
        "BytesWarning"
        | "DeprecationWarning"
        | "FutureWarning"
        | "ImportWarning"
        | "PendingDeprecationWarning"
        | "ResourceWarning"
        | "RuntimeWarning"
        | "SyntaxWarning"
        | "UnicodeWarning"
        | "UserWarning" => Some("Warning"),
        // 3.10+
        "EncodingWarning" => Some("Warning"),

        // BaseException itself has no superclass, and unknown names return None
        _ => None,
    }
}

/// Normalizes exception aliases to their canonical name
///
/// `IOError` and `EnvironmentError` are aliases for `OSError`
///
/// Keep in sync with the alias arms in [`builtin_exception_superclass`]
fn normalize_exception_alias(name: &str) -> &str {
    match name {
        "IOError" | "EnvironmentError" => "OSError",
        _ => name,
    }
}

/// Returns `true` if `ancestor` is an ancestor of `descendant` in the builtin
/// exception hierarchy
///
/// Returns `false` if either name is not a known builtin exception, or if
/// `ancestor` is not actually an ancestor of `descendant`
///
/// Handles exception aliases (`IOError`/`EnvironmentError` -> `OSError`) and
/// `ExceptionGroup`'s diamond inheritance (`Exception` + `BaseExceptionGroup`)
///
/// ```
/// use ruff_python_stdlib::builtins::is_builtin_exception_ancestor;
///
/// // Direct and multi-level ancestry
/// assert!(is_builtin_exception_ancestor("Exception", "ValueError"));
/// assert!(is_builtin_exception_ancestor("LookupError", "KeyError"));
/// assert!(is_builtin_exception_ancestor("BaseException", "ValueError"));
/// assert!(is_builtin_exception_ancestor("SyntaxError", "TabError"));
/// assert!(is_builtin_exception_ancestor("ValueError", "UnicodeDecodeError"));
/// assert!(is_builtin_exception_ancestor("OSError", "BrokenPipeError"));
///
/// // Not ancestors
/// assert!(!is_builtin_exception_ancestor("ValueError", "TypeError"));
/// assert!(!is_builtin_exception_ancestor("KeyError", "IndexError"));
/// assert!(!is_builtin_exception_ancestor("ValueError", "Exception"));
///
/// // Same class is not its own ancestor
/// assert!(!is_builtin_exception_ancestor("ValueError", "ValueError"));
///
/// // Aliases of the same class ARE treated as ancestor relationships
/// assert!(is_builtin_exception_ancestor("OSError", "IOError"));
/// assert!(is_builtin_exception_ancestor("IOError", "OSError"));
/// assert!(is_builtin_exception_ancestor("EnvironmentError", "IOError"));
///
/// // Aliases also work as ancestors of subclasses
/// assert!(is_builtin_exception_ancestor("IOError", "ConnectionError"));
/// assert!(is_builtin_exception_ancestor("EnvironmentError", "FileNotFoundError"));
///
/// // Unknown names
/// assert!(!is_builtin_exception_ancestor("MyError", "ValueError"));
/// assert!(!is_builtin_exception_ancestor("Exception", "MyError"));
///
/// // ExceptionGroup diamond inheritance
/// assert!(is_builtin_exception_ancestor("BaseExceptionGroup", "ExceptionGroup"));
/// assert!(is_builtin_exception_ancestor("Exception", "ExceptionGroup"));
/// assert!(is_builtin_exception_ancestor("BaseException", "ExceptionGroup"));
/// ```
pub fn is_builtin_exception_ancestor(ancestor: &str, descendant: &str) -> bool {
    let ancestor_normalized = normalize_exception_alias(ancestor);
    let descendant_normalized = normalize_exception_alias(descendant);

    // Aliases of the same class (e.g., OSError and IOError) are treated as
    // the ancestor catching the descendant
    if ancestor_normalized == descendant_normalized {
        return ancestor != descendant;
    }

    // ExceptionGroup has diamond inheritance: it extends both Exception and
    // BaseExceptionGroup. Walk both branches
    if descendant_normalized == "ExceptionGroup" && ancestor_normalized == "BaseExceptionGroup" {
        return true;
    }

    let mut current = descendant_normalized;
    loop {
        match builtin_exception_superclass(current) {
            Some(parent) if parent == ancestor_normalized => return true,
            Some(parent) => current = parent,
            None => return false,
        }
    }
}
