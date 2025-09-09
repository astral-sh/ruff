"""codecs -- Python Codec Registry, API and helpers.


Written by Marc-Andre Lemburg (mal@lemburg.com).

(c) Copyright CNRI, All Rights Reserved. NO WARRANTY.

"""

import sys
import types
from _codecs import *
from _typeshed import ReadableBuffer
from abc import abstractmethod
from collections.abc import Callable, Generator, Iterable
from typing import Any, BinaryIO, ClassVar, Final, Literal, Protocol, TextIO, overload, type_check_only
from typing_extensions import Self, TypeAlias, disjoint_base

__all__ = [
    "register",
    "lookup",
    "open",
    "EncodedFile",
    "BOM",
    "BOM_BE",
    "BOM_LE",
    "BOM32_BE",
    "BOM32_LE",
    "BOM64_BE",
    "BOM64_LE",
    "BOM_UTF8",
    "BOM_UTF16",
    "BOM_UTF16_LE",
    "BOM_UTF16_BE",
    "BOM_UTF32",
    "BOM_UTF32_LE",
    "BOM_UTF32_BE",
    "CodecInfo",
    "Codec",
    "IncrementalEncoder",
    "IncrementalDecoder",
    "StreamReader",
    "StreamWriter",
    "StreamReaderWriter",
    "StreamRecoder",
    "getencoder",
    "getdecoder",
    "getincrementalencoder",
    "getincrementaldecoder",
    "getreader",
    "getwriter",
    "encode",
    "decode",
    "iterencode",
    "iterdecode",
    "strict_errors",
    "ignore_errors",
    "replace_errors",
    "xmlcharrefreplace_errors",
    "backslashreplace_errors",
    "namereplace_errors",
    "register_error",
    "lookup_error",
]

BOM32_BE: Final = b"\xfe\xff"
BOM32_LE: Final = b"\xff\xfe"
BOM64_BE: Final = b"\x00\x00\xfe\xff"
BOM64_LE: Final = b"\xff\xfe\x00\x00"

_BufferedEncoding: TypeAlias = Literal[
    "idna",
    "raw-unicode-escape",
    "unicode-escape",
    "utf-16",
    "utf-16-be",
    "utf-16-le",
    "utf-32",
    "utf-32-be",
    "utf-32-le",
    "utf-7",
    "utf-8",
    "utf-8-sig",
]

@type_check_only
class _WritableStream(Protocol):
    def write(self, data: bytes, /) -> object: ...
    def seek(self, offset: int, whence: int, /) -> object: ...
    def close(self) -> object: ...

@type_check_only
class _ReadableStream(Protocol):
    def read(self, size: int = ..., /) -> bytes: ...
    def seek(self, offset: int, whence: int, /) -> object: ...
    def close(self) -> object: ...

@type_check_only
class _Stream(_WritableStream, _ReadableStream, Protocol): ...

# TODO: this only satisfies the most common interface, where
# bytes is the raw form and str is the cooked form.
# In the long run, both should become template parameters maybe?
# There *are* bytes->bytes and str->str encodings in the standard library.
# They were much more common in Python 2 than in Python 3.

@type_check_only
class _Encoder(Protocol):
    def __call__(self, input: str, errors: str = ..., /) -> tuple[bytes, int]: ...  # signature of Codec().encode

@type_check_only
class _Decoder(Protocol):
    def __call__(self, input: ReadableBuffer, errors: str = ..., /) -> tuple[str, int]: ...  # signature of Codec().decode

@type_check_only
class _StreamReader(Protocol):
    def __call__(self, stream: _ReadableStream, errors: str = ..., /) -> StreamReader: ...

@type_check_only
class _StreamWriter(Protocol):
    def __call__(self, stream: _WritableStream, errors: str = ..., /) -> StreamWriter: ...

@type_check_only
class _IncrementalEncoder(Protocol):
    def __call__(self, errors: str = ...) -> IncrementalEncoder: ...

@type_check_only
class _IncrementalDecoder(Protocol):
    def __call__(self, errors: str = ...) -> IncrementalDecoder: ...

@type_check_only
class _BufferedIncrementalDecoder(Protocol):
    def __call__(self, errors: str = ...) -> BufferedIncrementalDecoder: ...

if sys.version_info >= (3, 12):
    class CodecInfo(tuple[_Encoder, _Decoder, _StreamReader, _StreamWriter]):
        """Codec details when looking up the codec registry"""

        _is_text_encoding: bool
        @property
        def encode(self) -> _Encoder: ...
        @property
        def decode(self) -> _Decoder: ...
        @property
        def streamreader(self) -> _StreamReader: ...
        @property
        def streamwriter(self) -> _StreamWriter: ...
        @property
        def incrementalencoder(self) -> _IncrementalEncoder: ...
        @property
        def incrementaldecoder(self) -> _IncrementalDecoder: ...
        name: str
        def __new__(
            cls,
            encode: _Encoder,
            decode: _Decoder,
            streamreader: _StreamReader | None = None,
            streamwriter: _StreamWriter | None = None,
            incrementalencoder: _IncrementalEncoder | None = None,
            incrementaldecoder: _IncrementalDecoder | None = None,
            name: str | None = None,
            *,
            _is_text_encoding: bool | None = None,
        ) -> Self: ...

else:
    @disjoint_base
    class CodecInfo(tuple[_Encoder, _Decoder, _StreamReader, _StreamWriter]):
        """Codec details when looking up the codec registry"""

        _is_text_encoding: bool
        @property
        def encode(self) -> _Encoder: ...
        @property
        def decode(self) -> _Decoder: ...
        @property
        def streamreader(self) -> _StreamReader: ...
        @property
        def streamwriter(self) -> _StreamWriter: ...
        @property
        def incrementalencoder(self) -> _IncrementalEncoder: ...
        @property
        def incrementaldecoder(self) -> _IncrementalDecoder: ...
        name: str
        def __new__(
            cls,
            encode: _Encoder,
            decode: _Decoder,
            streamreader: _StreamReader | None = None,
            streamwriter: _StreamWriter | None = None,
            incrementalencoder: _IncrementalEncoder | None = None,
            incrementaldecoder: _IncrementalDecoder | None = None,
            name: str | None = None,
            *,
            _is_text_encoding: bool | None = None,
        ) -> Self: ...

def getencoder(encoding: str) -> _Encoder:
    """Lookup up the codec for the given encoding and return
    its encoder function.

    Raises a LookupError in case the encoding cannot be found.

    """

def getdecoder(encoding: str) -> _Decoder:
    """Lookup up the codec for the given encoding and return
    its decoder function.

    Raises a LookupError in case the encoding cannot be found.

    """

def getincrementalencoder(encoding: str) -> _IncrementalEncoder:
    """Lookup up the codec for the given encoding and return
    its IncrementalEncoder class or factory function.

    Raises a LookupError in case the encoding cannot be found
    or the codecs doesn't provide an incremental encoder.

    """

@overload
def getincrementaldecoder(encoding: _BufferedEncoding) -> _BufferedIncrementalDecoder:
    """Lookup up the codec for the given encoding and return
    its IncrementalDecoder class or factory function.

    Raises a LookupError in case the encoding cannot be found
    or the codecs doesn't provide an incremental decoder.

    """

@overload
def getincrementaldecoder(encoding: str) -> _IncrementalDecoder: ...
def getreader(encoding: str) -> _StreamReader:
    """Lookup up the codec for the given encoding and return
    its StreamReader class or factory function.

    Raises a LookupError in case the encoding cannot be found.

    """

def getwriter(encoding: str) -> _StreamWriter:
    """Lookup up the codec for the given encoding and return
    its StreamWriter class or factory function.

    Raises a LookupError in case the encoding cannot be found.

    """

def open(
    filename: str, mode: str = "r", encoding: str | None = None, errors: str = "strict", buffering: int = -1
) -> StreamReaderWriter:
    """Open an encoded file using the given mode and return
    a wrapped version providing transparent encoding/decoding.

    Note: The wrapped version will only accept the object format
    defined by the codecs, i.e. Unicode objects for most builtin
    codecs. Output is also codec dependent and will usually be
    Unicode as well.

    If encoding is not None, then the
    underlying encoded files are always opened in binary mode.
    The default file mode is 'r', meaning to open the file in read mode.

    encoding specifies the encoding which is to be used for the
    file.

    errors may be given to define the error handling. It defaults
    to 'strict' which causes ValueErrors to be raised in case an
    encoding error occurs.

    buffering has the same meaning as for the builtin open() API.
    It defaults to -1 which means that the default buffer size will
    be used.

    The returned wrapped file object provides an extra attribute
    .encoding which allows querying the used encoding. This
    attribute is only available if an encoding was specified as
    parameter.
    """

def EncodedFile(file: _Stream, data_encoding: str, file_encoding: str | None = None, errors: str = "strict") -> StreamRecoder:
    """Return a wrapped version of file which provides transparent
    encoding translation.

    Data written to the wrapped file is decoded according
    to the given data_encoding and then encoded to the underlying
    file using file_encoding. The intermediate data type
    will usually be Unicode but depends on the specified codecs.

    Bytes read from the file are decoded using file_encoding and then
    passed back to the caller encoded using data_encoding.

    If file_encoding is not given, it defaults to data_encoding.

    errors may be given to define the error handling. It defaults
    to 'strict' which causes ValueErrors to be raised in case an
    encoding error occurs.

    The returned wrapped file object provides two extra attributes
    .data_encoding and .file_encoding which reflect the given
    parameters of the same name. The attributes can be used for
    introspection by Python programs.

    """

def iterencode(iterator: Iterable[str], encoding: str, errors: str = "strict") -> Generator[bytes, None, None]:
    """
    Encoding iterator.

    Encodes the input strings from the iterator using an IncrementalEncoder.

    errors and kwargs are passed through to the IncrementalEncoder
    constructor.
    """

def iterdecode(iterator: Iterable[bytes], encoding: str, errors: str = "strict") -> Generator[str, None, None]:
    """
    Decoding iterator.

    Decodes the input strings from the iterator using an IncrementalDecoder.

    errors and kwargs are passed through to the IncrementalDecoder
    constructor.
    """

BOM: Final[Literal[b"\xff\xfe", b"\xfe\xff"]]  # depends on `sys.byteorder`
BOM_BE: Final = b"\xfe\xff"
BOM_LE: Final = b"\xff\xfe"
BOM_UTF8: Final = b"\xef\xbb\xbf"
BOM_UTF16: Final[Literal[b"\xff\xfe", b"\xfe\xff"]]  # depends on `sys.byteorder`
BOM_UTF16_BE: Final = b"\xfe\xff"
BOM_UTF16_LE: Final = b"\xff\xfe"
BOM_UTF32: Final[Literal[b"\xff\xfe\x00\x00", b"\x00\x00\xfe\xff"]]  # depends on `sys.byteorder`
BOM_UTF32_BE: Final = b"\x00\x00\xfe\xff"
BOM_UTF32_LE: Final = b"\xff\xfe\x00\x00"

def strict_errors(exception: UnicodeError, /) -> tuple[str | bytes, int]:
    """Implements the 'strict' error handling, which raises a UnicodeError on coding errors."""

def replace_errors(exception: UnicodeError, /) -> tuple[str | bytes, int]:
    """Implements the 'replace' error handling, which replaces malformed data with a replacement marker."""

def ignore_errors(exception: UnicodeError, /) -> tuple[str | bytes, int]:
    """Implements the 'ignore' error handling, which ignores malformed data and continues."""

def xmlcharrefreplace_errors(exception: UnicodeError, /) -> tuple[str | bytes, int]:
    """Implements the 'xmlcharrefreplace' error handling, which replaces an unencodable character with the appropriate XML character reference."""

def backslashreplace_errors(exception: UnicodeError, /) -> tuple[str | bytes, int]:
    """Implements the 'backslashreplace' error handling, which replaces malformed data with a backslashed escape sequence."""

def namereplace_errors(exception: UnicodeError, /) -> tuple[str | bytes, int]:
    """Implements the 'namereplace' error handling, which replaces an unencodable character with a \\N{...} escape sequence."""

class Codec:
    """Defines the interface for stateless encoders/decoders.

    The .encode()/.decode() methods may use different error
    handling schemes by providing the errors argument. These
    string values are predefined:

     'strict' - raise a ValueError error (or a subclass)
     'ignore' - ignore the character and continue with the next
     'replace' - replace with a suitable replacement character;
                Python will use the official U+FFFD REPLACEMENT
                CHARACTER for the builtin Unicode codecs on
                decoding and '?' on encoding.
     'surrogateescape' - replace with private code points U+DCnn.
     'xmlcharrefreplace' - Replace with the appropriate XML
                           character reference (only for encoding).
     'backslashreplace'  - Replace with backslashed escape sequences.
     'namereplace'       - Replace with \\N{...} escape sequences
                           (only for encoding).

    The set of allowed values can be extended via register_error.

    """

    # These are sort of @abstractmethod but sort of not.
    # The StreamReader and StreamWriter subclasses only implement one.
    def encode(self, input: str, errors: str = "strict") -> tuple[bytes, int]:
        """Encodes the object input and returns a tuple (output
        object, length consumed).

        errors defines the error handling to apply. It defaults to
        'strict' handling.

        The method may not store state in the Codec instance. Use
        StreamWriter for codecs which have to keep state in order to
        make encoding efficient.

        The encoder must be able to handle zero length input and
        return an empty object of the output object type in this
        situation.

        """

    def decode(self, input: bytes, errors: str = "strict") -> tuple[str, int]:
        """Decodes the object input and returns a tuple (output
        object, length consumed).

        input must be an object which provides the bf_getreadbuf
        buffer slot. Python strings, buffer objects and memory
        mapped files are examples of objects providing this slot.

        errors defines the error handling to apply. It defaults to
        'strict' handling.

        The method may not store state in the Codec instance. Use
        StreamReader for codecs which have to keep state in order to
        make decoding efficient.

        The decoder must be able to handle zero length input and
        return an empty object of the output object type in this
        situation.

        """

class IncrementalEncoder:
    """
    An IncrementalEncoder encodes an input in multiple steps. The input can
    be passed piece by piece to the encode() method. The IncrementalEncoder
    remembers the state of the encoding process between calls to encode().
    """

    errors: str
    def __init__(self, errors: str = "strict") -> None:
        """
        Creates an IncrementalEncoder instance.

        The IncrementalEncoder may use different error handling schemes by
        providing the errors keyword argument. See the module docstring
        for a list of possible values.
        """

    @abstractmethod
    def encode(self, input: str, final: bool = False) -> bytes:
        """
        Encodes input and returns the resulting object.
        """

    def reset(self) -> None:
        """
        Resets the encoder to the initial state.
        """
    # documentation says int but str is needed for the subclass.
    def getstate(self) -> int | str:
        """
        Return the current state of the encoder.
        """

    def setstate(self, state: int | str) -> None:
        """
        Set the current state of the encoder. state must have been
        returned by getstate().
        """

class IncrementalDecoder:
    """
    An IncrementalDecoder decodes an input in multiple steps. The input can
    be passed piece by piece to the decode() method. The IncrementalDecoder
    remembers the state of the decoding process between calls to decode().
    """

    errors: str
    def __init__(self, errors: str = "strict") -> None:
        """
        Create an IncrementalDecoder instance.

        The IncrementalDecoder may use different error handling schemes by
        providing the errors keyword argument. See the module docstring
        for a list of possible values.
        """

    @abstractmethod
    def decode(self, input: ReadableBuffer, final: bool = False) -> str:
        """
        Decode input and returns the resulting object.
        """

    def reset(self) -> None:
        """
        Reset the decoder to the initial state.
        """

    def getstate(self) -> tuple[bytes, int]:
        """
        Return the current state of the decoder.

        This must be a (buffered_input, additional_state_info) tuple.
        buffered_input must be a bytes object containing bytes that
        were passed to decode() that have not yet been converted.
        additional_state_info must be a non-negative integer
        representing the state of the decoder WITHOUT yet having
        processed the contents of buffered_input.  In the initial state
        and after reset(), getstate() must return (b"", 0).
        """

    def setstate(self, state: tuple[bytes, int]) -> None:
        """
        Set the current state of the decoder.

        state must have been returned by getstate().  The effect of
        setstate((b"", 0)) must be equivalent to reset().
        """

# These are not documented but used in encodings/*.py implementations.
class BufferedIncrementalEncoder(IncrementalEncoder):
    """
    This subclass of IncrementalEncoder can be used as the baseclass for an
    incremental encoder if the encoder must keep some of the output in a
    buffer between calls to encode().
    """

    buffer: str
    def __init__(self, errors: str = "strict") -> None: ...
    @abstractmethod
    def _buffer_encode(self, input: str, errors: str, final: bool) -> tuple[bytes, int]: ...
    def encode(self, input: str, final: bool = False) -> bytes: ...

class BufferedIncrementalDecoder(IncrementalDecoder):
    """
    This subclass of IncrementalDecoder can be used as the baseclass for an
    incremental decoder if the decoder must be able to handle incomplete
    byte sequences.
    """

    buffer: bytes
    def __init__(self, errors: str = "strict") -> None: ...
    @abstractmethod
    def _buffer_decode(self, input: ReadableBuffer, errors: str, final: bool) -> tuple[str, int]: ...
    def decode(self, input: ReadableBuffer, final: bool = False) -> str: ...

# TODO: it is not possible to specify the requirement that all other
# attributes and methods are passed-through from the stream.
class StreamWriter(Codec):
    stream: _WritableStream
    errors: str
    def __init__(self, stream: _WritableStream, errors: str = "strict") -> None:
        """Creates a StreamWriter instance.

        stream must be a file-like object open for writing.

        The StreamWriter may use different error handling
        schemes by providing the errors keyword argument. These
        parameters are predefined:

         'strict' - raise a ValueError (or a subclass)
         'ignore' - ignore the character and continue with the next
         'replace'- replace with a suitable replacement character
         'xmlcharrefreplace' - Replace with the appropriate XML
                               character reference.
         'backslashreplace'  - Replace with backslashed escape
                               sequences.
         'namereplace'       - Replace with \\N{...} escape sequences.

        The set of allowed parameter values can be extended via
        register_error.
        """

    def write(self, object: str) -> None:
        """Writes the object's contents encoded to self.stream."""

    def writelines(self, list: Iterable[str]) -> None:
        """Writes the concatenated list of strings to the stream
        using .write().
        """

    def reset(self) -> None:
        """Resets the codec buffers used for keeping internal state.

        Calling this method should ensure that the data on the
        output is put into a clean state, that allows appending
        of new fresh data without having to rescan the whole
        stream to recover state.

        """

    def seek(self, offset: int, whence: int = 0) -> None: ...
    def __enter__(self) -> Self: ...
    def __exit__(self, type: type[BaseException] | None, value: BaseException | None, tb: types.TracebackType | None) -> None: ...
    def __getattr__(self, name: str, getattr: Callable[[Any, str], Any] = ...) -> Any:
        """Inherit all other methods from the underlying stream."""

class StreamReader(Codec):
    stream: _ReadableStream
    errors: str
    # This is set to str, but some subclasses set to bytes instead.
    charbuffertype: ClassVar[type] = ...
    def __init__(self, stream: _ReadableStream, errors: str = "strict") -> None:
        """Creates a StreamReader instance.

        stream must be a file-like object open for reading.

        The StreamReader may use different error handling
        schemes by providing the errors keyword argument. These
        parameters are predefined:

         'strict' - raise a ValueError (or a subclass)
         'ignore' - ignore the character and continue with the next
         'replace'- replace with a suitable replacement character
         'backslashreplace' - Replace with backslashed escape sequences;

        The set of allowed parameter values can be extended via
        register_error.
        """

    def read(self, size: int = -1, chars: int = -1, firstline: bool = False) -> str:
        """Decodes data from the stream self.stream and returns the
        resulting object.

        chars indicates the number of decoded code points or bytes to
        return. read() will never return more data than requested,
        but it might return less, if there is not enough available.

        size indicates the approximate maximum number of decoded
        bytes or code points to read for decoding. The decoder
        can modify this setting as appropriate. The default value
        -1 indicates to read and decode as much as possible.  size
        is intended to prevent having to decode huge files in one
        step.

        If firstline is true, and a UnicodeDecodeError happens
        after the first line terminator in the input only the first line
        will be returned, the rest of the input will be kept until the
        next call to read().

        The method should use a greedy read strategy, meaning that
        it should read as much data as is allowed within the
        definition of the encoding and the given size, e.g.  if
        optional encoding endings or state markers are available
        on the stream, these should be read too.
        """

    def readline(self, size: int | None = None, keepends: bool = True) -> str:
        """Read one line from the input stream and return the
        decoded data.

        size, if given, is passed as size argument to the
        read() method.

        """

    def readlines(self, sizehint: int | None = None, keepends: bool = True) -> list[str]:
        """Read all lines available on the input stream
        and return them as a list.

        Line breaks are implemented using the codec's decoder
        method and are included in the list entries.

        sizehint, if given, is ignored since there is no efficient
        way of finding the true end-of-line.

        """

    def reset(self) -> None:
        """Resets the codec buffers used for keeping internal state.

        Note that no stream repositioning should take place.
        This method is primarily intended to be able to recover
        from decoding errors.

        """

    def seek(self, offset: int, whence: int = 0) -> None:
        """Set the input stream's current position.

        Resets the codec buffers used for keeping state.
        """

    def __enter__(self) -> Self: ...
    def __exit__(self, type: type[BaseException] | None, value: BaseException | None, tb: types.TracebackType | None) -> None: ...
    def __iter__(self) -> Self: ...
    def __next__(self) -> str:
        """Return the next decoded line from the input stream."""

    def __getattr__(self, name: str, getattr: Callable[[Any, str], Any] = ...) -> Any:
        """Inherit all other methods from the underlying stream."""

# Doesn't actually inherit from TextIO, but wraps a BinaryIO to provide text reading and writing
# and delegates attributes to the underlying binary stream with __getattr__.
class StreamReaderWriter(TextIO):
    """StreamReaderWriter instances allow wrapping streams which
    work in both read and write modes.

    The design is such that one can use the factory functions
    returned by the codec.lookup() function to construct the
    instance.

    """

    stream: _Stream
    def __init__(self, stream: _Stream, Reader: _StreamReader, Writer: _StreamWriter, errors: str = "strict") -> None:
        """Creates a StreamReaderWriter instance.

        stream must be a Stream-like object.

        Reader, Writer must be factory functions or classes
        providing the StreamReader, StreamWriter interface resp.

        Error handling is done in the same way as defined for the
        StreamWriter/Readers.

        """

    def read(self, size: int = -1) -> str: ...
    def readline(self, size: int | None = None) -> str: ...
    def readlines(self, sizehint: int | None = None) -> list[str]: ...
    def __next__(self) -> str:
        """Return the next decoded line from the input stream."""

    def __iter__(self) -> Self: ...
    def write(self, data: str) -> None: ...  # type: ignore[override]
    def writelines(self, list: Iterable[str]) -> None: ...
    def reset(self) -> None: ...
    def seek(self, offset: int, whence: int = 0) -> None: ...  # type: ignore[override]
    def __enter__(self) -> Self: ...
    def __exit__(self, type: type[BaseException] | None, value: BaseException | None, tb: types.TracebackType | None) -> None: ...
    def __getattr__(self, name: str) -> Any:
        """Inherit all other methods from the underlying stream."""
    # These methods don't actually exist directly, but they are needed to satisfy the TextIO
    # interface. At runtime, they are delegated through __getattr__.
    def close(self) -> None: ...
    def fileno(self) -> int: ...
    def flush(self) -> None: ...
    def isatty(self) -> bool: ...
    def readable(self) -> bool: ...
    def truncate(self, size: int | None = ...) -> int: ...
    def seekable(self) -> bool: ...
    def tell(self) -> int: ...
    def writable(self) -> bool: ...

class StreamRecoder(BinaryIO):
    """StreamRecoder instances translate data from one encoding to another.

    They use the complete set of APIs returned by the
    codecs.lookup() function to implement their task.

    Data written to the StreamRecoder is first decoded into an
    intermediate format (depending on the "decode" codec) and then
    written to the underlying stream using an instance of the provided
    Writer class.

    In the other direction, data is read from the underlying stream using
    a Reader instance and then encoded and returned to the caller.

    """

    data_encoding: str
    file_encoding: str
    def __init__(
        self,
        stream: _Stream,
        encode: _Encoder,
        decode: _Decoder,
        Reader: _StreamReader,
        Writer: _StreamWriter,
        errors: str = "strict",
    ) -> None:
        """Creates a StreamRecoder instance which implements a two-way
        conversion: encode and decode work on the frontend (the
        data visible to .read() and .write()) while Reader and Writer
        work on the backend (the data in stream).

        You can use these objects to do transparent
        transcodings from e.g. latin-1 to utf-8 and back.

        stream must be a file-like object.

        encode and decode must adhere to the Codec interface; Reader and
        Writer must be factory functions or classes providing the
        StreamReader and StreamWriter interfaces resp.

        Error handling is done in the same way as defined for the
        StreamWriter/Readers.

        """

    def read(self, size: int = -1) -> bytes: ...
    def readline(self, size: int | None = None) -> bytes: ...
    def readlines(self, sizehint: int | None = None) -> list[bytes]: ...
    def __next__(self) -> bytes:
        """Return the next decoded line from the input stream."""

    def __iter__(self) -> Self: ...
    # Base class accepts more types than just bytes
    def write(self, data: bytes) -> None: ...  # type: ignore[override]
    def writelines(self, list: Iterable[bytes]) -> None: ...  # type: ignore[override]
    def reset(self) -> None: ...
    def __getattr__(self, name: str) -> Any:
        """Inherit all other methods from the underlying stream."""

    def __enter__(self) -> Self: ...
    def __exit__(self, type: type[BaseException] | None, value: BaseException | None, tb: types.TracebackType | None) -> None: ...
    def seek(self, offset: int, whence: int = 0) -> None: ...  # type: ignore[override]
    # These methods don't actually exist directly, but they are needed to satisfy the BinaryIO
    # interface. At runtime, they are delegated through __getattr__.
    def close(self) -> None: ...
    def fileno(self) -> int: ...
    def flush(self) -> None: ...
    def isatty(self) -> bool: ...
    def readable(self) -> bool: ...
    def truncate(self, size: int | None = ...) -> int: ...
    def seekable(self) -> bool: ...
    def tell(self) -> int: ...
    def writable(self) -> bool: ...
