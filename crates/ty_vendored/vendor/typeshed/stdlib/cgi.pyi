"""Support module for CGI (Common Gateway Interface) scripts.

This module defines a number of utilities for use by CGI scripts
written in Python.

The global variable maxlen can be set to an integer indicating the maximum size
of a POST request. POST requests larger than this size will result in a
ValueError being raised during parsing. The default value of this variable is 0,
meaning the request size is unlimited.
"""

import os
from _typeshed import SupportsContainsAndGetItem, SupportsGetItem, SupportsItemAccess, Unused
from builtins import list as _list, type as _type
from collections.abc import Iterable, Iterator, Mapping
from email.message import Message
from types import TracebackType
from typing import IO, Any, Protocol, type_check_only
from typing_extensions import Self

__all__ = [
    "MiniFieldStorage",
    "FieldStorage",
    "parse",
    "parse_multipart",
    "parse_header",
    "test",
    "print_exception",
    "print_environ",
    "print_form",
    "print_directory",
    "print_arguments",
    "print_environ_usage",
]

def parse(
    fp: IO[Any] | None = None,
    environ: SupportsItemAccess[str, str] = os.environ,
    keep_blank_values: bool = ...,
    strict_parsing: bool = ...,
    separator: str = "&",
) -> dict[str, list[str]]:
    """Parse a query in the environment or from a file (default stdin)

    Arguments, all optional:

    fp              : file pointer; default: sys.stdin.buffer

    environ         : environment dictionary; default: os.environ

    keep_blank_values: flag indicating whether blank values in
        percent-encoded forms should be treated as blank strings.
        A true value indicates that blanks should be retained as
        blank strings.  The default false value indicates that
        blank values are to be ignored and treated as if they were
        not included.

    strict_parsing: flag indicating what to do with parsing errors.
        If false (the default), errors are silently ignored.
        If true, errors raise a ValueError exception.

    separator: str. The symbol to use for separating the query arguments.
        Defaults to &.
    """

def parse_multipart(
    fp: IO[Any], pdict: SupportsGetItem[str, bytes], encoding: str = "utf-8", errors: str = "replace", separator: str = "&"
) -> dict[str, list[Any]]:
    """Parse multipart input.

    Arguments:
    fp   : input file
    pdict: dictionary containing other parameters of content-type header
    encoding, errors: request encoding and error handler, passed to
        FieldStorage

    Returns a dictionary just like parse_qs(): keys are the field names, each
    value is a list of values for that field. For non-file fields, the value
    is a list of strings.
    """

@type_check_only
class _Environ(Protocol):
    def __getitem__(self, k: str, /) -> str: ...
    def keys(self) -> Iterable[str]: ...

def parse_header(line: str) -> tuple[str, dict[str, str]]:
    """Parse a Content-type like header.

    Return the main content-type and a dictionary of options.

    """

def test(environ: _Environ = os.environ) -> None:
    """Robust test CGI script, usable as main program.

    Write minimal HTTP headers and dump all information provided to
    the script in HTML form.

    """

def print_environ(environ: _Environ = os.environ) -> None:
    """Dump the shell environment as HTML."""

def print_form(form: dict[str, Any]) -> None:
    """Dump the contents of a form as HTML."""

def print_directory() -> None:
    """Dump the current directory as HTML."""

def print_environ_usage() -> None:
    """Dump a list of environment variables used by CGI as HTML."""

class MiniFieldStorage:
    """Like FieldStorage, for use when no file uploads are possible."""

    # The first five "Any" attributes here are always None, but mypy doesn't support that
    filename: Any
    list: Any
    type: Any
    file: IO[bytes] | None
    type_options: dict[Any, Any]
    disposition: Any
    disposition_options: dict[Any, Any]
    headers: dict[Any, Any]
    name: Any
    value: Any
    def __init__(self, name: Any, value: Any) -> None:
        """Constructor from field name and value."""

class FieldStorage:
    """Store a sequence of fields, reading multipart/form-data.

    This class provides naming, typing, files stored on disk, and
    more.  At the top level, it is accessible like a dictionary, whose
    keys are the field names.  (Note: None can occur as a field name.)
    The items are either a Python list (if there's multiple values) or
    another FieldStorage or MiniFieldStorage object.  If it's a single
    object, it has the following attributes:

    name: the field name, if specified; otherwise None

    filename: the filename, if specified; otherwise None; this is the
        client side filename, *not* the file name on which it is
        stored (that's a temporary file you don't deal with)

    value: the value as a *string*; for file uploads, this
        transparently reads the file every time you request the value
        and returns *bytes*

    file: the file(-like) object from which you can read the data *as
        bytes* ; None if the data is stored a simple string

    type: the content-type, or None if not specified

    type_options: dictionary of options specified on the content-type
        line

    disposition: content-disposition, or None if not specified

    disposition_options: dictionary of corresponding options

    headers: a dictionary(-like) object (sometimes email.message.Message or a
        subclass thereof) containing *all* headers

    The class is subclassable, mostly for the purpose of overriding
    the make_file() method, which is called internally to come up with
    a file open for reading and writing.  This makes it possible to
    override the default choice of storing all files in a temporary
    directory and unlinking them as soon as they have been opened.

    """

    FieldStorageClass: _type | None
    keep_blank_values: int
    strict_parsing: int
    qs_on_post: str | None
    headers: Mapping[str, str] | Message
    fp: IO[bytes]
    encoding: str
    errors: str
    outerboundary: bytes
    bytes_read: int
    limit: int | None
    disposition: str
    disposition_options: dict[str, str]
    filename: str | None
    file: IO[bytes] | None
    type: str
    type_options: dict[str, str]
    innerboundary: bytes
    length: int
    done: int
    list: _list[Any] | None
    value: None | bytes | _list[Any]
    def __init__(
        self,
        fp: IO[Any] | None = None,
        headers: Mapping[str, str] | Message | None = None,
        outerboundary: bytes = b"",
        environ: SupportsContainsAndGetItem[str, str] = os.environ,
        keep_blank_values: int = 0,
        strict_parsing: int = 0,
        limit: int | None = None,
        encoding: str = "utf-8",
        errors: str = "replace",
        max_num_fields: int | None = None,
        separator: str = "&",
    ) -> None:
        """Constructor.  Read multipart/* until last part.

        Arguments, all optional:

        fp              : file pointer; default: sys.stdin.buffer
            (not used when the request method is GET)
            Can be :
            1. a TextIOWrapper object
            2. an object whose read() and readline() methods return bytes

        headers         : header dictionary-like object; default:
            taken from environ as per CGI spec

        outerboundary   : terminating multipart boundary
            (for internal use only)

        environ         : environment dictionary; default: os.environ

        keep_blank_values: flag indicating whether blank values in
            percent-encoded forms should be treated as blank strings.
            A true value indicates that blanks should be retained as
            blank strings.  The default false value indicates that
            blank values are to be ignored and treated as if they were
            not included.

        strict_parsing: flag indicating what to do with parsing errors.
            If false (the default), errors are silently ignored.
            If true, errors raise a ValueError exception.

        limit : used internally to read parts of multipart/form-data forms,
            to exit from the reading loop when reached. It is the difference
            between the form content-length and the number of bytes already
            read

        encoding, errors : the encoding and error handler used to decode the
            binary stream to strings. Must be the same as the charset defined
            for the page sending the form (content-type : meta http-equiv or
            header)

        max_num_fields: int. If set, then __init__ throws a ValueError
            if there are more than n fields read by parse_qsl().

        """

    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...
    def __iter__(self) -> Iterator[str]: ...
    def __getitem__(self, key: str) -> Any:
        """Dictionary style indexing."""

    def getvalue(self, key: str, default: Any = None) -> Any:
        """Dictionary style get() method, including 'value' lookup."""

    def getfirst(self, key: str, default: Any = None) -> Any:
        """Return the first value received."""

    def getlist(self, key: str) -> _list[Any]:
        """Return list of received values."""

    def keys(self) -> _list[str]:
        """Dictionary style keys() method."""

    def __contains__(self, key: str) -> bool:
        """Dictionary style __contains__ method."""

    def __len__(self) -> int:
        """Dictionary style len(x) support."""

    def __bool__(self) -> bool: ...
    def __del__(self) -> None: ...
    # Returns bytes or str IO depending on an internal flag
    def make_file(self) -> IO[Any]:
        """Overridable: return a readable & writable file.

        The file will be used as follows:
        - data is written to it
        - seek(0)
        - data is read from it

        The file is opened in binary mode for files, in text mode
        for other fields

        This version opens a temporary file for reading and writing,
        and immediately deletes (unlinks) it.  The trick (on Unix!) is
        that the file can still be used, but it can't be opened by
        another process, and it will automatically be deleted when it
        is closed or when the current process terminates.

        If you want a more permanent file, you derive a class which
        overrides this method.  If you want a visible temporary file
        that is nevertheless automatically deleted when the script
        terminates, try defining a __del__ method in a derived class
        which unlinks the temporary files you have created.

        """

def print_exception(
    type: type[BaseException] | None = None,
    value: BaseException | None = None,
    tb: TracebackType | None = None,
    limit: int | None = None,
) -> None: ...
def print_arguments() -> None: ...
