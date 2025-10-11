"""The io module provides the Python interfaces to stream handling. The
builtin open function is defined in this module.

At the top of the I/O hierarchy is the abstract base class IOBase. It
defines the basic interface to a stream. Note, however, that there is no
separation between reading and writing to streams; implementations are
allowed to raise an OSError if they do not support a given operation.

Extending IOBase is RawIOBase which deals simply with the reading and
writing of raw bytes to a stream. FileIO subclasses RawIOBase to provide
an interface to OS files.

BufferedIOBase deals with buffering on a raw byte stream (RawIOBase). Its
subclasses, BufferedWriter, BufferedReader, and BufferedRWPair buffer
streams that are readable, writable, and both respectively.
BufferedRandom provides a buffered interface to random access
streams. BytesIO is a simple stream of in-memory bytes.

Another IOBase subclass, TextIOBase, deals with the encoding and decoding
of streams into text. TextIOWrapper, which extends it, is a buffered text
interface to a buffered raw stream (`BufferedIOBase`). Finally, StringIO
is an in-memory stream for text.

Argument names are not part of the specification, and only the arguments
of open() are intended to be used as keyword arguments.

data:

DEFAULT_BUFFER_SIZE

   An int containing the default buffer size used by the module's buffered
   I/O classes.
"""

import builtins
import codecs
import sys
from _typeshed import FileDescriptorOrPath, MaybeNone, ReadableBuffer, WriteableBuffer
from collections.abc import Callable, Iterable, Iterator
from io import BufferedIOBase, RawIOBase, TextIOBase, UnsupportedOperation as UnsupportedOperation
from os import _Opener
from types import TracebackType
from typing import IO, Any, BinaryIO, Final, Generic, Literal, Protocol, TextIO, TypeVar, overload, type_check_only
from typing_extensions import Self, disjoint_base

_T = TypeVar("_T")

if sys.version_info >= (3, 14):
    DEFAULT_BUFFER_SIZE: Final = 131072
else:
    DEFAULT_BUFFER_SIZE: Final = 8192

open = builtins.open

def open_code(path: str) -> IO[bytes]:
    """Opens the provided file with the intent to import the contents.

    This may perform extra validation beyond open(), but is otherwise interchangeable
    with calling open(path, 'rb').
    """

BlockingIOError = builtins.BlockingIOError

if sys.version_info >= (3, 12):
    @disjoint_base
    class _IOBase:
        """The abstract base class for all I/O classes.

        This class provides dummy implementations for many methods that
        derived classes can override selectively; the default implementations
        represent a file that cannot be read, written or seeked.

        Even though IOBase does not declare read, readinto, or write because
        their signatures will vary, implementations and clients should
        consider those methods part of the interface. Also, implementations
        may raise UnsupportedOperation when operations they do not support are
        called.

        The basic type used for binary data read from or written to a file is
        bytes. Other bytes-like objects are accepted as method arguments too.
        In some cases (such as readinto), a writable object is required. Text
        I/O classes work with str data.

        Note that calling any method (except additional calls to close(),
        which are ignored) on a closed stream should raise a ValueError.

        IOBase (and its subclasses) support the iterator protocol, meaning
        that an IOBase object can be iterated over yielding the lines in a
        stream.

        IOBase also supports the :keyword:`with` statement. In this example,
        fp is closed after the suite of the with statement is complete:

        with open('spam.txt', 'r') as fp:
            fp.write('Spam and eggs!')
        """

        def __iter__(self) -> Iterator[bytes]:
            """Implement iter(self)."""

        def __next__(self) -> bytes:
            """Implement next(self)."""

        def __enter__(self) -> Self: ...
        def __exit__(
            self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
        ) -> None: ...
        def close(self) -> None:
            """Flush and close the IO object.

            This method has no effect if the file is already closed.
            """

        def fileno(self) -> int:
            """Return underlying file descriptor if one exists.

            Raise OSError if the IO object does not use a file descriptor.
            """

        def flush(self) -> None:
            """Flush write buffers, if applicable.

            This is not implemented for read-only and non-blocking streams.
            """

        def isatty(self) -> bool:
            """Return whether this is an 'interactive' stream.

            Return False if it can't be determined.
            """

        def readable(self) -> bool:
            """Return whether object was opened for reading.

            If False, read() will raise OSError.
            """
        read: Callable[..., Any]
        def readlines(self, hint: int = -1, /) -> list[bytes]:
            """Return a list of lines from the stream.

            hint can be specified to control the number of lines read: no more
            lines will be read if the total size (in bytes/characters) of all
            lines so far exceeds hint.
            """

        def seek(self, offset: int, whence: int = 0, /) -> int:
            """Change the stream position to the given byte offset.

              offset
                The stream position, relative to 'whence'.
              whence
                The relative position to seek from.

            The offset is interpreted relative to the position indicated by whence.
            Values for whence are:

            * os.SEEK_SET or 0 -- start of stream (the default); offset should be zero or positive
            * os.SEEK_CUR or 1 -- current stream position; offset may be negative
            * os.SEEK_END or 2 -- end of stream; offset is usually negative

            Return the new absolute position.
            """

        def seekable(self) -> bool:
            """Return whether object supports random access.

            If False, seek(), tell() and truncate() will raise OSError.
            This method may need to do a test seek().
            """

        def tell(self) -> int:
            """Return current stream position."""

        def truncate(self, size: int | None = None, /) -> int:
            """Truncate file to size bytes.

            File pointer is left unchanged. Size defaults to the current IO position
            as reported by tell(). Return the new size.
            """

        def writable(self) -> bool:
            """Return whether object was opened for writing.

            If False, write() will raise OSError.
            """
        write: Callable[..., Any]
        def writelines(self, lines: Iterable[ReadableBuffer], /) -> None:
            """Write a list of lines to stream.

            Line separators are not added, so it is usual for each of the
            lines provided to have a line separator at the end.
            """

        def readline(self, size: int | None = -1, /) -> bytes:
            """Read and return a line from the stream.

            If size is specified, at most size bytes will be read.

            The line terminator is always b'\\n' for binary files; for text
            files, the newlines argument to open can be used to select the line
            terminator(s) recognized.
            """

        def __del__(self) -> None:
            """Called when the instance is about to be destroyed."""

        @property
        def closed(self) -> bool: ...
        def _checkClosed(self) -> None: ...  # undocumented

else:
    class _IOBase:
        """The abstract base class for all I/O classes.

        This class provides dummy implementations for many methods that
        derived classes can override selectively; the default implementations
        represent a file that cannot be read, written or seeked.

        Even though IOBase does not declare read, readinto, or write because
        their signatures will vary, implementations and clients should
        consider those methods part of the interface. Also, implementations
        may raise UnsupportedOperation when operations they do not support are
        called.

        The basic type used for binary data read from or written to a file is
        bytes. Other bytes-like objects are accepted as method arguments too.
        In some cases (such as readinto), a writable object is required. Text
        I/O classes work with str data.

        Note that calling any method (except additional calls to close(),
        which are ignored) on a closed stream should raise a ValueError.

        IOBase (and its subclasses) support the iterator protocol, meaning
        that an IOBase object can be iterated over yielding the lines in a
        stream.

        IOBase also supports the :keyword:`with` statement. In this example,
        fp is closed after the suite of the with statement is complete:

        with open('spam.txt', 'r') as fp:
            fp.write('Spam and eggs!')
        """

        def __iter__(self) -> Iterator[bytes]:
            """Implement iter(self)."""

        def __next__(self) -> bytes:
            """Implement next(self)."""

        def __enter__(self) -> Self: ...
        def __exit__(
            self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
        ) -> None: ...
        def close(self) -> None:
            """Flush and close the IO object.

            This method has no effect if the file is already closed.
            """

        def fileno(self) -> int:
            """Returns underlying file descriptor if one exists.

            OSError is raised if the IO object does not use a file descriptor.
            """

        def flush(self) -> None:
            """Flush write buffers, if applicable.

            This is not implemented for read-only and non-blocking streams.
            """

        def isatty(self) -> bool:
            """Return whether this is an 'interactive' stream.

            Return False if it can't be determined.
            """

        def readable(self) -> bool:
            """Return whether object was opened for reading.

            If False, read() will raise OSError.
            """
        read: Callable[..., Any]
        def readlines(self, hint: int = -1, /) -> list[bytes]:
            """Return a list of lines from the stream.

            hint can be specified to control the number of lines read: no more
            lines will be read if the total size (in bytes/characters) of all
            lines so far exceeds hint.
            """

        def seek(self, offset: int, whence: int = 0, /) -> int:
            """Change the stream position to the given byte offset.

              offset
                The stream position, relative to 'whence'.
              whence
                The relative position to seek from.

            The offset is interpreted relative to the position indicated by whence.
            Values for whence are:

            * os.SEEK_SET or 0 -- start of stream (the default); offset should be zero or positive
            * os.SEEK_CUR or 1 -- current stream position; offset may be negative
            * os.SEEK_END or 2 -- end of stream; offset is usually negative

            Return the new absolute position.
            """

        def seekable(self) -> bool:
            """Return whether object supports random access.

            If False, seek(), tell() and truncate() will raise OSError.
            This method may need to do a test seek().
            """

        def tell(self) -> int:
            """Return current stream position."""

        def truncate(self, size: int | None = None, /) -> int:
            """Truncate file to size bytes.

            File pointer is left unchanged.  Size defaults to the current IO
            position as reported by tell().  Returns the new size.
            """

        def writable(self) -> bool:
            """Return whether object was opened for writing.

            If False, write() will raise OSError.
            """
        write: Callable[..., Any]
        def writelines(self, lines: Iterable[ReadableBuffer], /) -> None:
            """Write a list of lines to stream.

            Line separators are not added, so it is usual for each of the
            lines provided to have a line separator at the end.
            """

        def readline(self, size: int | None = -1, /) -> bytes:
            """Read and return a line from the stream.

            If size is specified, at most size bytes will be read.

            The line terminator is always b'\\n' for binary files; for text
            files, the newlines argument to open can be used to select the line
            terminator(s) recognized.
            """

        def __del__(self) -> None: ...
        @property
        def closed(self) -> bool: ...
        def _checkClosed(self) -> None: ...  # undocumented

class _RawIOBase(_IOBase):
    """Base class for raw binary I/O."""

    def readall(self) -> bytes:
        """Read until EOF, using multiple read() call."""
    # The following methods can return None if the file is in non-blocking mode
    # and no data is available.
    def readinto(self, buffer: WriteableBuffer, /) -> int | MaybeNone: ...
    def write(self, b: ReadableBuffer, /) -> int | MaybeNone: ...
    def read(self, size: int = -1, /) -> bytes | MaybeNone: ...

class _BufferedIOBase(_IOBase):
    """Base class for buffered IO objects.

    The main difference with RawIOBase is that the read() method
    supports omitting the size argument, and does not have a default
    implementation that defers to readinto().

    In addition, read(), readinto() and write() may raise
    BlockingIOError if the underlying raw stream is in non-blocking
    mode and not ready; unlike their raw counterparts, they will never
    return None.

    A typical implementation should not inherit from a RawIOBase
    implementation, but wrap one.
    """

    def detach(self) -> RawIOBase:
        """Disconnect this buffer from its underlying raw stream and return it.

        After the raw stream has been detached, the buffer is in an unusable
        state.
        """

    def readinto(self, buffer: WriteableBuffer, /) -> int: ...
    def write(self, buffer: ReadableBuffer, /) -> int:
        """Write buffer b to the IO stream.

        Return the number of bytes written, which is always
        the length of b in bytes.

        Raise BlockingIOError if the buffer is full and the
        underlying raw stream cannot accept more data at the moment.
        """

    def readinto1(self, buffer: WriteableBuffer, /) -> int: ...
    def read(self, size: int | None = -1, /) -> bytes:
        """Read and return up to n bytes.

        If the size argument is omitted, None, or negative, read and
        return all data until EOF.

        If the size argument is positive, and the underlying raw stream is
        not 'interactive', multiple raw reads may be issued to satisfy
        the byte count (unless EOF is reached first).
        However, for interactive raw streams (as well as sockets and pipes),
        at most one raw read will be issued, and a short result does not
        imply that EOF is imminent.

        Return an empty bytes object on EOF.

        Return None if the underlying raw stream was open in non-blocking
        mode and no data is available at the moment.
        """

    def read1(self, size: int = -1, /) -> bytes:
        """Read and return up to size bytes, with at most one read() call to the underlying raw stream.

        Return an empty bytes object on EOF.
        A short result does not imply that EOF is imminent.
        """

@disjoint_base
class FileIO(RawIOBase, _RawIOBase, BinaryIO):  # type: ignore[misc]  # incompatible definitions of writelines in the base classes
    """Open a file.

    The mode can be 'r' (default), 'w', 'x' or 'a' for reading,
    writing, exclusive creation or appending.  The file will be created if it
    doesn't exist when opened for writing or appending; it will be truncated
    when opened for writing.  A FileExistsError will be raised if it already
    exists when opened for creating. Opening a file for creating implies
    writing so this mode behaves in a similar way to 'w'.Add a '+' to the mode
    to allow simultaneous reading and writing. A custom opener can be used by
    passing a callable as *opener*. The underlying file descriptor for the file
    object is then obtained by calling opener with (*name*, *flags*).
    *opener* must return an open file descriptor (passing os.open as *opener*
    results in functionality similar to passing None).
    """

    mode: str
    # The type of "name" equals the argument passed in to the constructor,
    # but that can make FileIO incompatible with other I/O types that assume
    # "name" is a str. In the future, making FileIO generic might help.
    name: Any
    def __init__(
        self, file: FileDescriptorOrPath, mode: str = "r", closefd: bool = True, opener: _Opener | None = None
    ) -> None: ...
    @property
    def closefd(self) -> bool:
        """True if the file descriptor will be closed by close()."""

    def seek(self, pos: int, whence: int = 0, /) -> int:
        """Move to new file position and return the file position.

        Argument offset is a byte count.  Optional argument whence defaults to
        SEEK_SET or 0 (offset from start of file, offset should be >= 0); other values
        are SEEK_CUR or 1 (move relative to current position, positive or negative),
        and SEEK_END or 2 (move relative to end of file, usually negative, although
        many platforms allow seeking beyond the end of a file).

        Note that not all file objects are seekable.
        """

    def read(self, size: int | None = -1, /) -> bytes | MaybeNone:
        """Read at most size bytes, returned as bytes.

        If size is less than 0, read all bytes in the file making multiple read calls.
        See ``FileIO.readall``.

        Attempts to make only one system call, retrying only per PEP 475 (EINTR). This
        means less data may be returned than requested.

        In non-blocking mode, returns None if no data is available. Return an empty
        bytes object at EOF.
        """

@disjoint_base
class BytesIO(BufferedIOBase, _BufferedIOBase, BinaryIO):  # type: ignore[misc]  # incompatible definitions of methods in the base classes
    """Buffered I/O implementation using an in-memory bytes buffer."""

    def __init__(self, initial_bytes: ReadableBuffer = b"") -> None: ...
    # BytesIO does not contain a "name" field. This workaround is necessary
    # to allow BytesIO sub-classes to add this field, as it is defined
    # as a read-only property on IO[].
    name: Any
    def getvalue(self) -> bytes:
        """Retrieve the entire contents of the BytesIO object."""

    def getbuffer(self) -> memoryview:
        """Get a read-write view over the contents of the BytesIO object."""

    def read1(self, size: int | None = -1, /) -> bytes:
        """Read at most size bytes, returned as a bytes object.

        If the size argument is negative or omitted, read until EOF is reached.
        Return an empty bytes object at EOF.
        """

    def readlines(self, size: int | None = None, /) -> list[bytes]:
        """List of bytes objects, each a line from the file.

        Call readline() repeatedly and return a list of the lines so read.
        The optional size argument, if given, is an approximate bound on the
        total number of bytes in the lines returned.
        """

    def seek(self, pos: int, whence: int = 0, /) -> int:
        """Change stream position.

        Seek to byte offset pos relative to position indicated by whence:
             0  Start of stream (the default).  pos should be >= 0;
             1  Current position - pos may be negative;
             2  End of stream - pos usually negative.
        Returns the new absolute position.
        """

@type_check_only
class _BufferedReaderStream(Protocol):
    def read(self, n: int = ..., /) -> bytes: ...
    # Optional: def readall(self) -> bytes: ...
    def readinto(self, b: memoryview, /) -> int | None: ...
    def seek(self, pos: int, whence: int, /) -> int: ...
    def tell(self) -> int: ...
    def truncate(self, size: int, /) -> int: ...
    def flush(self) -> object: ...
    def close(self) -> object: ...
    @property
    def closed(self) -> bool: ...
    def readable(self) -> bool: ...
    def seekable(self) -> bool: ...

    # The following methods just pass through to the underlying stream. Since
    # not all streams support them, they are marked as optional here, and will
    # raise an AttributeError if called on a stream that does not support them.

    # @property
    # def name(self) -> Any: ...  # Type is inconsistent between the various I/O types.
    # @property
    # def mode(self) -> str: ...
    # def fileno(self) -> int: ...
    # def isatty(self) -> bool: ...

_BufferedReaderStreamT = TypeVar("_BufferedReaderStreamT", bound=_BufferedReaderStream, default=_BufferedReaderStream)

@disjoint_base
class BufferedReader(BufferedIOBase, _BufferedIOBase, BinaryIO, Generic[_BufferedReaderStreamT]):  # type: ignore[misc]  # incompatible definitions of methods in the base classes
    """Create a new buffered reader using the given readable raw IO object."""

    raw: _BufferedReaderStreamT
    if sys.version_info >= (3, 14):
        def __init__(self, raw: _BufferedReaderStreamT, buffer_size: int = 131072) -> None: ...
    else:
        def __init__(self, raw: _BufferedReaderStreamT, buffer_size: int = 8192) -> None: ...

    def peek(self, size: int = 0, /) -> bytes: ...
    def seek(self, target: int, whence: int = 0, /) -> int: ...
    def truncate(self, pos: int | None = None, /) -> int: ...

@disjoint_base
class BufferedWriter(BufferedIOBase, _BufferedIOBase, BinaryIO):  # type: ignore[misc]  # incompatible definitions of writelines in the base classes
    """A buffer for a writeable sequential RawIO object.

    The constructor creates a BufferedWriter for the given writeable raw
    stream. If the buffer_size is not given, it defaults to
    DEFAULT_BUFFER_SIZE.
    """

    raw: RawIOBase
    if sys.version_info >= (3, 14):
        def __init__(self, raw: RawIOBase, buffer_size: int = 131072) -> None: ...
    else:
        def __init__(self, raw: RawIOBase, buffer_size: int = 8192) -> None: ...

    def write(self, buffer: ReadableBuffer, /) -> int: ...
    def seek(self, target: int, whence: int = 0, /) -> int: ...
    def truncate(self, pos: int | None = None, /) -> int: ...

@disjoint_base
class BufferedRandom(BufferedIOBase, _BufferedIOBase, BinaryIO):  # type: ignore[misc]  # incompatible definitions of methods in the base classes
    """A buffered interface to random access streams.

    The constructor creates a reader and writer for a seekable stream,
    raw, given in the first argument. If the buffer_size is omitted it
    defaults to DEFAULT_BUFFER_SIZE.
    """

    mode: str
    name: Any
    raw: RawIOBase
    if sys.version_info >= (3, 14):
        def __init__(self, raw: RawIOBase, buffer_size: int = 131072) -> None: ...
    else:
        def __init__(self, raw: RawIOBase, buffer_size: int = 8192) -> None: ...

    def seek(self, target: int, whence: int = 0, /) -> int: ...  # stubtest needs this
    def peek(self, size: int = 0, /) -> bytes: ...
    def truncate(self, pos: int | None = None, /) -> int: ...

@disjoint_base
class BufferedRWPair(BufferedIOBase, _BufferedIOBase, Generic[_BufferedReaderStreamT]):
    """A buffered reader and writer object together.

    A buffered reader object and buffered writer object put together to
    form a sequential IO object that can read and write. This is typically
    used with a socket or two-way pipe.

    reader and writer are RawIOBase objects that are readable and
    writeable respectively. If the buffer_size is omitted it defaults to
    DEFAULT_BUFFER_SIZE.
    """

    if sys.version_info >= (3, 14):
        def __init__(self, reader: _BufferedReaderStreamT, writer: RawIOBase, buffer_size: int = 131072, /) -> None: ...
    else:
        def __init__(self, reader: _BufferedReaderStreamT, writer: RawIOBase, buffer_size: int = 8192, /) -> None: ...

    def peek(self, size: int = 0, /) -> bytes: ...

class _TextIOBase(_IOBase):
    """Base class for text I/O.

    This class provides a character and line based interface to stream
    I/O. There is no readinto method because Python's character strings
    are immutable.
    """

    encoding: str
    errors: str | None
    newlines: str | tuple[str, ...] | None
    def __iter__(self) -> Iterator[str]:  # type: ignore[override]
        """Implement iter(self)."""

    def __next__(self) -> str:  # type: ignore[override]
        """Implement next(self)."""

    def detach(self) -> BinaryIO:
        """Separate the underlying buffer from the TextIOBase and return it.

        After the underlying buffer has been detached, the TextIO is in an unusable state.
        """

    def write(self, s: str, /) -> int:
        """Write string s to stream.

        Return the number of characters written
        (which is always equal to the length of the string).
        """

    def writelines(self, lines: Iterable[str], /) -> None:  # type: ignore[override]
        """Write a list of lines to stream.

        Line separators are not added, so it is usual for each of the
        lines provided to have a line separator at the end.
        """

    def readline(self, size: int = -1, /) -> str:  # type: ignore[override]
        """Read until newline or EOF.

        Return an empty string if EOF is hit immediately.
        If size is specified, at most size characters will be read.
        """

    def readlines(self, hint: int = -1, /) -> list[str]:  # type: ignore[override]
        """Return a list of lines from the stream.

        hint can be specified to control the number of lines read: no more
        lines will be read if the total size (in bytes/characters) of all
        lines so far exceeds hint.
        """

    def read(self, size: int | None = -1, /) -> str:
        """Read at most size characters from stream.

        Read from underlying buffer until we have size characters or we hit EOF.
        If size is negative or omitted, read until EOF.
        """

@type_check_only
class _WrappedBuffer(Protocol):
    # "name" is wrapped by TextIOWrapper. Its type is inconsistent between
    # the various I/O types.
    @property
    def name(self) -> Any: ...
    @property
    def closed(self) -> bool: ...
    def read(self, size: int = ..., /) -> ReadableBuffer: ...
    # Optional: def read1(self, size: int, /) -> ReadableBuffer: ...
    def write(self, b: bytes, /) -> object: ...
    def flush(self) -> object: ...
    def close(self) -> object: ...
    def seekable(self) -> bool: ...
    def readable(self) -> bool: ...
    def writable(self) -> bool: ...
    def truncate(self, size: int, /) -> int: ...
    def fileno(self) -> int: ...
    def isatty(self) -> bool: ...
    # Optional: Only needs to be present if seekable() returns True.
    # def seek(self, offset: Literal[0], whence: Literal[2]) -> int: ...
    # def tell(self) -> int: ...

_BufferT_co = TypeVar("_BufferT_co", bound=_WrappedBuffer, default=_WrappedBuffer, covariant=True)

@disjoint_base
class TextIOWrapper(TextIOBase, _TextIOBase, TextIO, Generic[_BufferT_co]):  # type: ignore[misc]  # incompatible definitions of write in the base classes
    """Character and line based layer over a BufferedIOBase object, buffer.

    encoding gives the name of the encoding that the stream will be
    decoded or encoded with. It defaults to locale.getencoding().

    errors determines the strictness of encoding and decoding (see
    help(codecs.Codec) or the documentation for codecs.register) and
    defaults to "strict".

    newline controls how line endings are handled. It can be None, '',
    '\\n', '\\r', and '\\r\\n'.  It works as follows:

    * On input, if newline is None, universal newlines mode is
      enabled. Lines in the input can end in '\\n', '\\r', or '\\r\\n', and
      these are translated into '\\n' before being returned to the
      caller. If it is '', universal newline mode is enabled, but line
      endings are returned to the caller untranslated. If it has any of
      the other legal values, input lines are only terminated by the given
      string, and the line ending is returned to the caller untranslated.

    * On output, if newline is None, any '\\n' characters written are
      translated to the system default line separator, os.linesep. If
      newline is '' or '\\n', no translation takes place. If newline is any
      of the other legal values, any '\\n' characters written are translated
      to the given string.

    If line_buffering is True, a call to flush is implied when a call to
    write contains a newline character.
    """

    def __init__(
        self,
        buffer: _BufferT_co,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
        line_buffering: bool = False,
        write_through: bool = False,
    ) -> None: ...
    # Equals the "buffer" argument passed in to the constructor.
    @property
    def buffer(self) -> _BufferT_co: ...  # type: ignore[override]
    @property
    def line_buffering(self) -> bool: ...
    @property
    def write_through(self) -> bool: ...
    def reconfigure(
        self,
        *,
        encoding: str | None = None,
        errors: str | None = None,
        newline: str | None = None,
        line_buffering: bool | None = None,
        write_through: bool | None = None,
    ) -> None:
        """Reconfigure the text stream with new parameters.

        This also does an implicit stream flush.
        """

    def readline(self, size: int = -1, /) -> str: ...  # type: ignore[override]
    # Equals the "buffer" argument passed in to the constructor.
    def detach(self) -> _BufferT_co: ...  # type: ignore[override]
    # TextIOWrapper's version of seek only supports a limited subset of
    # operations.
    def seek(self, cookie: int, whence: int = 0, /) -> int:
        """Set the stream position, and return the new stream position.

          cookie
            Zero or an opaque number returned by tell().
          whence
            The relative position to seek from.

        Four operations are supported, given by the following argument
        combinations:

        - seek(0, SEEK_SET): Rewind to the start of the stream.
        - seek(cookie, SEEK_SET): Restore a previous position;
          'cookie' must be a number returned by tell().
        - seek(0, SEEK_END): Fast-forward to the end of the stream.
        - seek(0, SEEK_CUR): Leave the current stream position unchanged.

        Any other argument combinations are invalid,
        and may raise exceptions.
        """

    def truncate(self, pos: int | None = None, /) -> int: ...

@disjoint_base
class StringIO(TextIOBase, _TextIOBase, TextIO):  # type: ignore[misc]  # incompatible definitions of write in the base classes
    """Text I/O implementation using an in-memory buffer.

    The initial_value argument sets the value of object.  The newline
    argument is like the one of TextIOWrapper's constructor.
    """

    def __init__(self, initial_value: str | None = "", newline: str | None = "\n") -> None: ...
    # StringIO does not contain a "name" field. This workaround is necessary
    # to allow StringIO sub-classes to add this field, as it is defined
    # as a read-only property on IO[].
    name: Any
    def getvalue(self) -> str:
        """Retrieve the entire contents of the object."""

    @property
    def line_buffering(self) -> bool: ...
    def seek(self, pos: int, whence: int = 0, /) -> int:
        """Change stream position.

        Seek to character offset pos relative to position indicated by whence:
            0  Start of stream (the default).  pos should be >= 0;
            1  Current position - pos must be 0;
            2  End of stream - pos must be 0.
        Returns the new absolute position.
        """

    def truncate(self, pos: int | None = None, /) -> int:
        """Truncate size to pos.

        The pos argument defaults to the current file position, as
        returned by tell().  The current file position is unchanged.
        Returns the new absolute position.
        """

@disjoint_base
class IncrementalNewlineDecoder:
    """Codec used when reading a file in universal newlines mode.

    It wraps another incremental decoder, translating \\r\\n and \\r into \\n.
    It also records the types of newlines encountered.  When used with
    translate=False, it ensures that the newline sequence is returned in
    one piece. When used with decoder=None, it expects unicode strings as
    decode input and translates newlines without first invoking an external
    decoder.
    """

    def __init__(self, decoder: codecs.IncrementalDecoder | None, translate: bool, errors: str = "strict") -> None: ...
    def decode(self, input: ReadableBuffer | str, final: bool = False) -> str: ...
    @property
    def newlines(self) -> str | tuple[str, ...] | None: ...
    def getstate(self) -> tuple[bytes, int]: ...
    def reset(self) -> None: ...
    def setstate(self, state: tuple[bytes, int], /) -> None: ...

if sys.version_info >= (3, 10):
    @overload
    def text_encoding(encoding: None, stacklevel: int = 2, /) -> Literal["locale", "utf-8"]:
        """A helper function to choose the text encoding.

        When encoding is not None, this function returns it.
        Otherwise, this function returns the default text encoding
        (i.e. "locale" or "utf-8" depends on UTF-8 mode).

        This function emits an EncodingWarning if encoding is None and
        sys.flags.warn_default_encoding is true.

        This can be used in APIs with an encoding=None parameter.
        However, please consider using encoding="utf-8" for new APIs.
        """

    @overload
    def text_encoding(encoding: _T, stacklevel: int = 2, /) -> _T: ...
