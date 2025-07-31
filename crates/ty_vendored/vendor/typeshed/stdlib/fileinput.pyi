"""Helper class to quickly write a loop over all standard input files.

Typical use is:

    import fileinput
    for line in fileinput.input(encoding="utf-8"):
        process(line)

This iterates over the lines of all files listed in sys.argv[1:],
defaulting to sys.stdin if the list is empty.  If a filename is '-' it
is also replaced by sys.stdin and the optional arguments mode and
openhook are ignored.  To specify an alternative list of filenames,
pass it as the argument to input().  A single file name is also allowed.

Functions filename(), lineno() return the filename and cumulative line
number of the line that has just been read; filelineno() returns its
line number in the current file; isfirstline() returns true iff the
line just read is the first line of its file; isstdin() returns true
iff the line was read from sys.stdin.  Function nextfile() closes the
current file so that the next iteration will read the first line from
the next file (if any); lines not read from the file will not count
towards the cumulative line count; the filename is not changed until
after the first line of the next file has been read.  Function close()
closes the sequence.

Before any lines have been read, filename() returns None and both line
numbers are zero; nextfile() has no effect.  After all lines have been
read, filename() and the line number functions return the values
pertaining to the last line read; nextfile() has no effect.

All files are opened in text mode by default, you can override this by
setting the mode parameter to input() or FileInput.__init__().
If an I/O error occurs during opening or reading a file, the OSError
exception is raised.

If sys.stdin is used more than once, the second and further use will
return no lines, except perhaps for interactive use, or if it has been
explicitly reset (e.g. using sys.stdin.seek(0)).

Empty files are opened and immediately closed; the only time their
presence in the list of filenames is noticeable at all is when the
last file opened is empty.

It is possible that the last line of a file doesn't end in a newline
character; otherwise lines are returned including the trailing
newline.

Class FileInput is the implementation; its methods filename(),
lineno(), fileline(), isfirstline(), isstdin(), nextfile() and close()
correspond to the functions in the module.  In addition it has a
readline() method which returns the next input line, and a
__getitem__() method which implements the sequence behavior.  The
sequence must be accessed in strictly sequential order; sequence
access and readline() cannot be mixed.

Optional in-place filtering: if the keyword argument inplace=True is
passed to input() or to the FileInput constructor, the file is moved
to a backup file and standard output is directed to the input file.
This makes it possible to write a filter that rewrites its input file
in place.  If the keyword argument backup=".<some extension>" is also
given, it specifies the extension for the backup file, and the backup
file remains around; by default, the extension is ".bak" and it is
deleted when the output file is closed.  In-place filtering is
disabled when standard input is read.  XXX The current implementation
does not work for MS-DOS 8+3 filesystems.
"""

import sys
from _typeshed import AnyStr_co, StrOrBytesPath
from collections.abc import Callable, Iterable
from types import GenericAlias, TracebackType
from typing import IO, Any, AnyStr, Generic, Literal, Protocol, overload, type_check_only
from typing_extensions import Self, TypeAlias

__all__ = [
    "input",
    "close",
    "nextfile",
    "filename",
    "lineno",
    "filelineno",
    "fileno",
    "isfirstline",
    "isstdin",
    "FileInput",
    "hook_compressed",
    "hook_encoded",
]

if sys.version_info >= (3, 11):
    _TextMode: TypeAlias = Literal["r"]
else:
    _TextMode: TypeAlias = Literal["r", "rU", "U"]

@type_check_only
class _HasReadlineAndFileno(Protocol[AnyStr_co]):
    def readline(self) -> AnyStr_co: ...
    def fileno(self) -> int: ...

if sys.version_info >= (3, 10):
    # encoding and errors are added
    @overload
    def input(
        files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
        inplace: bool = False,
        backup: str = "",
        *,
        mode: _TextMode = "r",
        openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[str]] | None = None,
        encoding: str | None = None,
        errors: str | None = None,
    ) -> FileInput[str]:
        """Return an instance of the FileInput class, which can be iterated.

        The parameters are passed to the constructor of the FileInput class.
        The returned instance, in addition to being an iterator,
        keeps global state for the functions of this module,.
        """

    @overload
    def input(
        files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
        inplace: bool = False,
        backup: str = "",
        *,
        mode: Literal["rb"],
        openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[bytes]] | None = None,
        encoding: None = None,
        errors: None = None,
    ) -> FileInput[bytes]: ...
    @overload
    def input(
        files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
        inplace: bool = False,
        backup: str = "",
        *,
        mode: str,
        openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[Any]] | None = None,
        encoding: str | None = None,
        errors: str | None = None,
    ) -> FileInput[Any]: ...

else:
    # bufsize is dropped and mode and openhook become keyword-only
    @overload
    def input(
        files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
        inplace: bool = False,
        backup: str = "",
        *,
        mode: _TextMode = "r",
        openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[str]] | None = None,
    ) -> FileInput[str]:
        """Return an instance of the FileInput class, which can be iterated.

        The parameters are passed to the constructor of the FileInput class.
        The returned instance, in addition to being an iterator,
        keeps global state for the functions of this module,.
        """

    @overload
    def input(
        files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
        inplace: bool = False,
        backup: str = "",
        *,
        mode: Literal["rb"],
        openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[bytes]] | None = None,
    ) -> FileInput[bytes]: ...
    @overload
    def input(
        files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
        inplace: bool = False,
        backup: str = "",
        *,
        mode: str,
        openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[Any]] | None = None,
    ) -> FileInput[Any]: ...

def close() -> None:
    """Close the sequence."""

def nextfile() -> None:
    """
    Close the current file so that the next iteration will read the first
    line from the next file (if any); lines not read from the file will
    not count towards the cumulative line count. The filename is not
    changed until after the first line of the next file has been read.
    Before the first line has been read, this function has no effect;
    it cannot be used to skip the first file. After the last line of the
    last file has been read, this function has no effect.
    """

def filename() -> str:
    """
    Return the name of the file currently being read.
    Before the first line has been read, returns None.
    """

def lineno() -> int:
    """
    Return the cumulative line number of the line that has just been read.
    Before the first line has been read, returns 0. After the last line
    of the last file has been read, returns the line number of that line.
    """

def filelineno() -> int:
    """
    Return the line number in the current file. Before the first line
    has been read, returns 0. After the last line of the last file has
    been read, returns the line number of that line within the file.
    """

def fileno() -> int:
    """
    Return the file number of the current file. When no file is currently
    opened, returns -1.
    """

def isfirstline() -> bool:
    """
    Returns true the line just read is the first line of its file,
    otherwise returns false.
    """

def isstdin() -> bool:
    """
    Returns true if the last line was read from sys.stdin,
    otherwise returns false.
    """

class FileInput(Generic[AnyStr]):
    """FileInput([files[, inplace[, backup]]], *, mode=None, openhook=None)

    Class FileInput is the implementation of the module; its methods
    filename(), lineno(), fileline(), isfirstline(), isstdin(), fileno(),
    nextfile() and close() correspond to the functions of the same name
    in the module.
    In addition it has a readline() method which returns the next
    input line, and a __getitem__() method which implements the
    sequence behavior. The sequence must be accessed in strictly
    sequential order; random access and readline() cannot be mixed.
    """

    if sys.version_info >= (3, 10):
        # encoding and errors are added
        @overload
        def __init__(
            self: FileInput[str],
            files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
            inplace: bool = False,
            backup: str = "",
            *,
            mode: _TextMode = "r",
            openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[str]] | None = None,
            encoding: str | None = None,
            errors: str | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self: FileInput[bytes],
            files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
            inplace: bool = False,
            backup: str = "",
            *,
            mode: Literal["rb"],
            openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[bytes]] | None = None,
            encoding: None = None,
            errors: None = None,
        ) -> None: ...
        @overload
        def __init__(
            self: FileInput[Any],
            files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
            inplace: bool = False,
            backup: str = "",
            *,
            mode: str,
            openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[Any]] | None = None,
            encoding: str | None = None,
            errors: str | None = None,
        ) -> None: ...

    else:
        # bufsize is dropped and mode and openhook become keyword-only
        @overload
        def __init__(
            self: FileInput[str],
            files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
            inplace: bool = False,
            backup: str = "",
            *,
            mode: _TextMode = "r",
            openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[str]] | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self: FileInput[bytes],
            files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
            inplace: bool = False,
            backup: str = "",
            *,
            mode: Literal["rb"],
            openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[bytes]] | None = None,
        ) -> None: ...
        @overload
        def __init__(
            self: FileInput[Any],
            files: StrOrBytesPath | Iterable[StrOrBytesPath] | None = None,
            inplace: bool = False,
            backup: str = "",
            *,
            mode: str,
            openhook: Callable[[StrOrBytesPath, str], _HasReadlineAndFileno[Any]] | None = None,
        ) -> None: ...

    def __del__(self) -> None: ...
    def close(self) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> None: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> AnyStr: ...
    if sys.version_info < (3, 11):
        def __getitem__(self, i: int) -> AnyStr: ...

    def nextfile(self) -> None: ...
    def readline(self) -> AnyStr: ...
    def filename(self) -> str: ...
    def lineno(self) -> int: ...
    def filelineno(self) -> int: ...
    def fileno(self) -> int: ...
    def isfirstline(self) -> bool: ...
    def isstdin(self) -> bool: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """Represent a PEP 585 generic type

        E.g. for t = list[int], t.__origin__ is list and t.__args__ is (int,).
        """

if sys.version_info >= (3, 10):
    def hook_compressed(
        filename: StrOrBytesPath, mode: str, *, encoding: str | None = None, errors: str | None = None
    ) -> IO[Any]: ...

else:
    def hook_compressed(filename: StrOrBytesPath, mode: str) -> IO[Any]: ...

def hook_encoded(encoding: str, errors: str | None = None) -> Callable[[StrOrBytesPath, str], IO[Any]]: ...
