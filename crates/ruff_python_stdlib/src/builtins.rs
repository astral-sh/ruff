/// A list of all Python builtins.
///
/// Intended to be kept in sync with [`is_python_builtin`].
pub const PYTHON_BUILTINS: &[&str] = &[
    "ArithmeticError",
    "AssertionError",
    "AttributeError",
    "BaseException",
    "BaseExceptionGroup",
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
    "EncodingWarning",
    "EnvironmentError",
    "Exception",
    "ExceptionGroup",
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
    "aiter",
    "all",
    "anext",
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
pub const IPYTHON_BUILTINS: &[&str] = &["__IPYTHON__", "display", "get_ipython"];

/// Globally defined names which are not attributes of the builtins module, or
/// are only present on some platforms.
pub const MAGIC_GLOBALS: &[&str] = &[
    "WindowsError",
    "__annotations__",
    "__builtins__",
    "__cached__",
    "__file__",
];

/// Returns `true` if the given name is that of a Python builtin.
///
/// Intended to be kept in sync with [`PYTHON_BUILTINS`].
pub fn is_python_builtin(name: &str) -> bool {
    // Constructed by converting the `PYTHON_BUILTINS` slice to a `match` expression.
    matches!(
        name,
        "ArithmeticError"
            | "AssertionError"
            | "AttributeError"
            | "BaseException"
            | "BaseExceptionGroup"
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
            | "EncodingWarning"
            | "EnvironmentError"
            | "Exception"
            | "ExceptionGroup"
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
            | "aiter"
            | "all"
            | "anext"
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
    )
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
pub fn is_ipython_builtin(name: &str) -> bool {
    // Constructed by converting the `IPYTHON_BUILTINS` slice to a `match` expression.
    matches!(name, "__IPYTHON__" | "display" | "get_ipython")
}

/// Returns `true` if the given name is that of a builtin exception.
///
/// See: <https://docs.python.org/3/library/exceptions.html#exception-hierarchy>
pub fn is_exception(name: &str) -> bool {
    matches!(
        name,
        "BaseException"
            | "BaseExceptionGroup"
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
            | "ExceptionGroup"
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
            | "EncodingWarning"
            | "FutureWarning"
            | "ImportWarning"
            | "PendingDeprecationWarning"
            | "ResourceWarning"
            | "RuntimeWarning"
            | "SyntaxWarning"
            | "UnicodeWarning"
            | "UserWarning"
    )
}
