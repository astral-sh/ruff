import abc
import sys
from _io import (
    DEFAULT_BUFFER_SIZE as DEFAULT_BUFFER_SIZE,
    BlockingIOError as BlockingIOError,
    BufferedRandom as BufferedRandom,
    BufferedReader as BufferedReader,
    BufferedRWPair as BufferedRWPair,
    BufferedWriter as BufferedWriter,
    BytesIO as BytesIO,
    FileIO as FileIO,
    IncrementalNewlineDecoder as IncrementalNewlineDecoder,
    StringIO as StringIO,
    TextIOWrapper as TextIOWrapper,
    _BufferedIOBase,
    _IOBase,
    _RawIOBase,
    _TextIOBase,
    _WrappedBuffer as _WrappedBuffer,  # used elsewhere in typeshed
    open as open,
    open_code as open_code,
)
from typing import Final

__all__ = [
    "BlockingIOError",
    "open",
    "open_code",
    "IOBase",
    "RawIOBase",
    "FileIO",
    "BytesIO",
    "StringIO",
    "BufferedIOBase",
    "BufferedReader",
    "BufferedWriter",
    "BufferedRWPair",
    "BufferedRandom",
    "TextIOBase",
    "TextIOWrapper",
    "UnsupportedOperation",
    "SEEK_SET",
    "SEEK_CUR",
    "SEEK_END",
]

if sys.version_info >= (3, 11):
    from _io import text_encoding as text_encoding

    __all__ += ["DEFAULT_BUFFER_SIZE", "IncrementalNewlineDecoder", "text_encoding"]

SEEK_SET: Final = 0
SEEK_CUR: Final = 1
SEEK_END: Final = 2

class UnsupportedOperation(OSError, ValueError): ...
class IOBase(_IOBase, metaclass=abc.ABCMeta): ...
class RawIOBase(_RawIOBase, IOBase): ...
class BufferedIOBase(_BufferedIOBase, IOBase): ...
class TextIOBase(_TextIOBase, IOBase): ...
