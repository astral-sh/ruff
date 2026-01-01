"""Response classes used by urllib.

The base class, addbase, defines a minimal file-like interface,
including read() and readline().  The typical response object is an
addinfourl instance, which defines an info() method that returns
headers and a geturl() method that returns the url.
"""

import tempfile
from _typeshed import ReadableBuffer
from collections.abc import Callable, Iterable
from email.message import Message
from types import TracebackType
from typing import IO, Any

__all__ = ["addbase", "addclosehook", "addinfo", "addinfourl"]

class addbase(tempfile._TemporaryFileWrapper[bytes]):
    """Base class for addinfo and addclosehook. Is a good idea for garbage collection."""

    fp: IO[bytes]
    def __init__(self, fp: IO[bytes]) -> None: ...
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> None: ...
    # These methods don't actually exist, but the class inherits at runtime from
    # tempfile._TemporaryFileWrapper, which uses __getattr__ to delegate to the
    # underlying file object. To satisfy the BinaryIO interface, we pretend that this
    # class has these additional methods.
    def write(self, s: ReadableBuffer) -> int: ...
    def writelines(self, lines: Iterable[ReadableBuffer]) -> None: ...

class addclosehook(addbase):
    """Class to add a close hook to an open file."""

    closehook: Callable[..., object]
    hookargs: tuple[Any, ...]
    def __init__(self, fp: IO[bytes], closehook: Callable[..., object], *hookargs: Any) -> None: ...

class addinfo(addbase):
    """class to add an info() method to an open file."""

    headers: Message
    def __init__(self, fp: IO[bytes], headers: Message) -> None: ...
    def info(self) -> Message: ...

class addinfourl(addinfo):
    """class to add info() and geturl() methods to an open file."""

    url: str
    code: int | None
    @property
    def status(self) -> int | None: ...
    def __init__(self, fp: IO[bytes], headers: Message, url: str, code: int | None = None) -> None: ...
    def geturl(self) -> str: ...
    def getcode(self) -> int | None: ...
