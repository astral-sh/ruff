"""TELNET client class.

Based on RFC 854: TELNET Protocol Specification, by J. Postel and
J. Reynolds

Example:

>>> from telnetlib import Telnet
>>> tn = Telnet('www.python.org', 79)   # connect to finger port
>>> tn.write(b'guido\\r\\n')
>>> print(tn.read_all())
Login       Name               TTY         Idle    When    Where
guido    Guido van Rossum      pts/2        <Dec  2 11:10> snag.cnri.reston..

>>>

Note that read_all() won't read until eof -- it just reads some data
-- but it guarantees to read at least one byte unless EOF is hit.

It is possible to pass a Telnet object to a selector in order to wait until
more data is available.  Note that in this case, read_eager() may return b''
even if there was data on the socket, because the protocol negotiation may have
eaten the data.  This is why EOFError is needed in some cases to distinguish
between "no data" and "connection closed" (since the socket also appears ready
for reading when it is closed).

To do:
- option negotiation
- timeout should be intrinsic to the connection object instead of an
  option on one of the read calls only

"""

import socket
from collections.abc import Callable, MutableSequence, Sequence
from re import Match, Pattern
from types import TracebackType
from typing import Any
from typing_extensions import Self

__all__ = ["Telnet"]

DEBUGLEVEL: int
TELNET_PORT: int

IAC: bytes
DONT: bytes
DO: bytes
WONT: bytes
WILL: bytes
theNULL: bytes

SE: bytes
NOP: bytes
DM: bytes
BRK: bytes
IP: bytes
AO: bytes
AYT: bytes
EC: bytes
EL: bytes
GA: bytes
SB: bytes

BINARY: bytes
ECHO: bytes
RCP: bytes
SGA: bytes
NAMS: bytes
STATUS: bytes
TM: bytes
RCTE: bytes
NAOL: bytes
NAOP: bytes
NAOCRD: bytes
NAOHTS: bytes
NAOHTD: bytes
NAOFFD: bytes
NAOVTS: bytes
NAOVTD: bytes
NAOLFD: bytes
XASCII: bytes
LOGOUT: bytes
BM: bytes
DET: bytes
SUPDUP: bytes
SUPDUPOUTPUT: bytes
SNDLOC: bytes
TTYPE: bytes
EOR: bytes
TUID: bytes
OUTMRK: bytes
TTYLOC: bytes
VT3270REGIME: bytes
X3PAD: bytes
NAWS: bytes
TSPEED: bytes
LFLOW: bytes
LINEMODE: bytes
XDISPLOC: bytes
OLD_ENVIRON: bytes
AUTHENTICATION: bytes
ENCRYPT: bytes
NEW_ENVIRON: bytes

TN3270E: bytes
XAUTH: bytes
CHARSET: bytes
RSP: bytes
COM_PORT_OPTION: bytes
SUPPRESS_LOCAL_ECHO: bytes
TLS: bytes
KERMIT: bytes
SEND_URL: bytes
FORWARD_X: bytes
PRAGMA_LOGON: bytes
SSPI_LOGON: bytes
PRAGMA_HEARTBEAT: bytes
EXOPL: bytes
NOOPT: bytes

class Telnet:
    """Telnet interface class.

    An instance of this class represents a connection to a telnet
    server.  The instance is initially not connected; the open()
    method must be used to establish a connection.  Alternatively, the
    host name and optional port number can be passed to the
    constructor, too.

    Don't try to reopen an already connected instance.

    This class has many read_*() methods.  Note that some of them
    raise EOFError when the end of the connection is read, because
    they can return an empty string for other reasons.  See the
    individual doc strings.

    read_until(expected, [timeout])
        Read until the expected string has been seen, or a timeout is
        hit (default is no timeout); may block.

    read_all()
        Read all data until EOF; may block.

    read_some()
        Read at least one byte or EOF; may block.

    read_very_eager()
        Read all data available already queued or on the socket,
        without blocking.

    read_eager()
        Read either data already queued or some data available on the
        socket, without blocking.

    read_lazy()
        Read all data in the raw queue (processing it first), without
        doing any socket I/O.

    read_very_lazy()
        Reads all data in the cooked queue, without doing any socket
        I/O.

    read_sb_data()
        Reads available data between SB ... SE sequence. Don't block.

    set_option_negotiation_callback(callback)
        Each time a telnet option is read on the input flow, this callback
        (if set) is called with the following parameters :
        callback(telnet socket, command, option)
            option will be chr(0) when there is no option.
        No other action is done afterwards by telnetlib.

    """

    host: str | None  # undocumented
    sock: socket.socket | None  # undocumented
    def __init__(self, host: str | None = None, port: int = 0, timeout: float = ...) -> None:
        """Constructor.

        When called without arguments, create an unconnected instance.
        With a hostname argument, it connects the instance; port number
        and timeout are optional.
        """

    def open(self, host: str, port: int = 0, timeout: float = ...) -> None:
        """Connect to a host.

        The optional second argument is the port number, which
        defaults to the standard telnet port (23).

        Don't try to reopen an already connected instance.
        """

    def msg(self, msg: str, *args: Any) -> None:
        """Print a debug message, when the debug level is > 0.

        If extra arguments are present, they are substituted in the
        message using the standard string formatting operator.

        """

    def set_debuglevel(self, debuglevel: int) -> None:
        """Set the debug level.

        The higher it is, the more debug output you get (on sys.stdout).

        """

    def close(self) -> None:
        """Close the connection."""

    def get_socket(self) -> socket.socket:
        """Return the socket object used internally."""

    def fileno(self) -> int:
        """Return the fileno() of the socket object used internally."""

    def write(self, buffer: bytes) -> None:
        """Write a string to the socket, doubling any IAC characters.

        Can block if the connection is blocked.  May raise
        OSError if the connection is closed.

        """

    def read_until(self, match: bytes, timeout: float | None = None) -> bytes:
        """Read until a given string is encountered or until timeout.

        When no match is found, return whatever is available instead,
        possibly the empty string.  Raise EOFError if the connection
        is closed and no cooked data is available.

        """

    def read_all(self) -> bytes:
        """Read all data until EOF; block until connection closed."""

    def read_some(self) -> bytes:
        """Read at least one byte of cooked data unless EOF is hit.

        Return b'' if EOF is hit.  Block if no data is immediately
        available.

        """

    def read_very_eager(self) -> bytes:
        """Read everything that's possible without blocking in I/O (eager).

        Raise EOFError if connection closed and no cooked data
        available.  Return b'' if no cooked data available otherwise.
        Don't block unless in the midst of an IAC sequence.

        """

    def read_eager(self) -> bytes:
        """Read readily available data.

        Raise EOFError if connection closed and no cooked data
        available.  Return b'' if no cooked data available otherwise.
        Don't block unless in the midst of an IAC sequence.

        """

    def read_lazy(self) -> bytes:
        """Process and return data that's already in the queues (lazy).

        Raise EOFError if connection closed and no data available.
        Return b'' if no cooked data available otherwise.  Don't block
        unless in the midst of an IAC sequence.

        """

    def read_very_lazy(self) -> bytes:
        """Return any data available in the cooked queue (very lazy).

        Raise EOFError if connection closed and no data available.
        Return b'' if no cooked data available otherwise.  Don't block.

        """

    def read_sb_data(self) -> bytes:
        """Return any data available in the SB ... SE queue.

        Return b'' if no SB ... SE available. Should only be called
        after seeing a SB or SE command. When a new SB command is
        found, old unread SB data will be discarded. Don't block.

        """

    def set_option_negotiation_callback(self, callback: Callable[[socket.socket, bytes, bytes], object] | None) -> None:
        """Provide a callback function called after each receipt of a telnet option."""

    def process_rawq(self) -> None:
        """Transfer from raw queue to cooked queue.

        Set self.eof when connection is closed.  Don't block unless in
        the midst of an IAC sequence.

        """

    def rawq_getchar(self) -> bytes:
        """Get next char from raw queue.

        Block if no data is immediately available.  Raise EOFError
        when connection is closed.

        """

    def fill_rawq(self) -> None:
        """Fill raw queue from exactly one recv() system call.

        Block if no data is immediately available.  Set self.eof when
        connection is closed.

        """

    def sock_avail(self) -> bool:
        """Test whether data is available on the socket."""

    def interact(self) -> None:
        """Interaction function, emulates a very dumb telnet client."""

    def mt_interact(self) -> None:
        """Multithreaded version of interact()."""

    def listener(self) -> None:
        """Helper for mt_interact() -- this executes in the other thread."""

    def expect(
        self, list: MutableSequence[Pattern[bytes] | bytes] | Sequence[Pattern[bytes]], timeout: float | None = None
    ) -> tuple[int, Match[bytes] | None, bytes]:
        """Read until one from a list of a regular expressions matches.

        The first argument is a list of regular expressions, either
        compiled (re.Pattern instances) or uncompiled (strings).
        The optional second argument is a timeout, in seconds; default
        is no timeout.

        Return a tuple of three items: the index in the list of the
        first regular expression that matches; the re.Match object
        returned; and the text read up till and including the match.

        If EOF is read and no text was read, raise EOFError.
        Otherwise, when nothing matches, return (-1, None, text) where
        text is the text received so far (may be the empty string if a
        timeout happened).

        If a regular expression ends with a greedy match (e.g. '.*')
        or if more than one expression can match the same input, the
        results are undeterministic, and may depend on the I/O timing.

        """

    def __enter__(self) -> Self: ...
    def __exit__(
        self, type: type[BaseException] | None, value: BaseException | None, traceback: TracebackType | None
    ) -> None: ...
    def __del__(self) -> None:
        """Destructor -- close the connection."""
