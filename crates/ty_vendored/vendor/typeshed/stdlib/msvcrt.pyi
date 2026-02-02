import sys
from typing import Final

# This module is only available on Windows
if sys.platform == "win32":
    CRT_ASSEMBLY_VERSION: Final[str]
    LK_UNLCK: Final = 0
    LK_LOCK: Final = 1
    LK_NBLCK: Final = 2
    LK_RLCK: Final = 3
    LK_NBRLCK: Final = 4
    SEM_FAILCRITICALERRORS: Final = 0x0001
    SEM_NOALIGNMENTFAULTEXCEPT: Final = 0x0004
    SEM_NOGPFAULTERRORBOX: Final = 0x0002
    SEM_NOOPENFILEERRORBOX: Final = 0x8000
    def locking(fd: int, mode: int, nbytes: int, /) -> None:
        """Lock part of a file based on file descriptor fd from the C runtime.

        Raises OSError on failure. The locked region of the file extends from
        the current file position for nbytes bytes, and may continue beyond
        the end of the file. mode must be one of the LK_* constants listed
        below. Multiple regions in a file may be locked at the same time, but
        may not overlap. Adjacent regions are not merged; they must be unlocked
        individually.
        """

    def setmode(fd: int, mode: int, /) -> int:
        """Set the line-end translation mode for the file descriptor fd.

        To set it to text mode, flags should be os.O_TEXT; for binary, it
        should be os.O_BINARY.

        Return value is the previous mode.
        """

    def open_osfhandle(handle: int, flags: int, /) -> int:
        """Create a C runtime file descriptor from the file handle handle.

        The flags parameter should be a bitwise OR of os.O_APPEND, os.O_RDONLY,
        and os.O_TEXT. The returned file descriptor may be used as a parameter
        to os.fdopen() to create a file object.
        """

    def get_osfhandle(fd: int, /) -> int:
        """Return the file handle for the file descriptor fd.

        Raises OSError if fd is not recognized.
        """

    def kbhit() -> bool:
        """Returns a nonzero value if a keypress is waiting to be read. Otherwise, return 0."""

    def getch() -> bytes:
        """Read a keypress and return the resulting character as a byte string.

        Nothing is echoed to the console. This call will block if a keypress is
        not already available, but will not wait for Enter to be pressed. If the
        pressed key was a special function key, this will return '\\000' or
        '\\xe0'; the next call will return the keycode. The Control-C keypress
        cannot be read with this function.
        """

    def getwch() -> str:
        """Wide char variant of getch(), returning a Unicode value."""

    def getche() -> bytes:
        """Similar to getch(), but the keypress will be echoed if possible."""

    def getwche() -> str:
        """Wide char variant of getche(), returning a Unicode value."""

    def putch(char: bytes | bytearray, /) -> None:
        """Print the byte string char to the console without buffering."""

    def putwch(unicode_char: str, /) -> None:
        """Wide char variant of putch(), accepting a Unicode value."""

    def ungetch(char: bytes | bytearray, /) -> None:
        """Opposite of getch.

        Cause the byte string char to be "pushed back" into the
        console buffer; it will be the next character read by
        getch() or getche().
        """

    def ungetwch(unicode_char: str, /) -> None:
        """Wide char variant of ungetch(), accepting a Unicode value."""

    def heapmin() -> None:
        """Minimize the malloc() heap.

        Force the malloc() heap to clean itself up and return unused blocks
        to the operating system. On failure, this raises OSError.
        """

    def SetErrorMode(mode: int, /) -> int:
        """Wrapper around SetErrorMode."""
    if sys.version_info >= (3, 10):
        def GetErrorMode() -> int:  # undocumented
            """Wrapper around GetErrorMode."""
