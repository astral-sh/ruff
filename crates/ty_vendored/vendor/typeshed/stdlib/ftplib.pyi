"""An FTP client class and some helper functions.

Based on RFC 959: File Transfer Protocol (FTP), by J. Postel and J. Reynolds

Example:

>>> from ftplib import FTP
>>> ftp = FTP('ftp.python.org') # connect to host, default port
>>> ftp.login() # default, i.e.: user anonymous, passwd anonymous@
'230 Guest login ok, access restrictions apply.'
>>> ftp.retrlines('LIST') # list directory contents
total 9
drwxr-xr-x   8 root     wheel        1024 Jan  3  1994 .
drwxr-xr-x   8 root     wheel        1024 Jan  3  1994 ..
drwxr-xr-x   2 root     wheel        1024 Jan  3  1994 bin
drwxr-xr-x   2 root     wheel        1024 Jan  3  1994 etc
d-wxrwxr-x   2 ftp      wheel        1024 Sep  5 13:43 incoming
drwxr-xr-x   2 root     wheel        1024 Nov 17  1993 lib
drwxr-xr-x   6 1094     wheel        1024 Sep 13 19:07 pub
drwxr-xr-x   3 root     wheel        1024 Jan  3  1994 usr
-rw-r--r--   1 root     root          312 Aug  1  1994 welcome.msg
'226 Transfer complete.'
>>> ftp.quit()
'221 Goodbye.'
>>>

A nice test that reveals some of the network dialogue would be:
python ftplib.py -d localhost -l -p -l
"""

import sys
from _typeshed import SupportsRead, SupportsReadline
from collections.abc import Callable, Iterable, Iterator
from socket import socket
from ssl import SSLContext
from types import TracebackType
from typing import Any, Final, Literal, TextIO
from typing_extensions import Self

__all__ = ["FTP", "error_reply", "error_temp", "error_perm", "error_proto", "all_errors", "FTP_TLS"]

MSG_OOB: Final = 1
FTP_PORT: Final = 21
MAXLINE: Final = 8192
CRLF: Final = "\r\n"
B_CRLF: Final = b"\r\n"

class Error(Exception): ...
class error_reply(Error): ...
class error_temp(Error): ...
class error_perm(Error): ...
class error_proto(Error): ...

all_errors: tuple[type[Exception], ...]

class FTP:
    """An FTP client class.

    To create a connection, call the class using these arguments:
            host, user, passwd, acct, timeout, source_address, encoding

    The first four arguments are all strings, and have default value ''.
    The parameter ´timeout´ must be numeric and defaults to None if not
    passed, meaning that no timeout will be set on any ftp socket(s).
    If a timeout is passed, then this is now the default timeout for all ftp
    socket operations for this instance.
    The last parameter is the encoding of filenames, which defaults to utf-8.

    Then use self.connect() with optional host and port argument.

    To download a file, use ftp.retrlines('RETR ' + filename),
    or ftp.retrbinary() with slightly different arguments.
    To upload a file, use ftp.storlines() or ftp.storbinary(),
    which have an open file as argument (see their definitions
    below for details).
    The download/upload functions first issue appropriate TYPE
    and PORT or PASV commands.
    """

    debugging: int
    host: str
    port: int
    maxline: int
    sock: socket | None
    welcome: str | None
    passiveserver: int
    timeout: float | None
    af: int
    lastresp: str
    file: TextIO | None
    encoding: str
    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_val: BaseException | None, exc_tb: TracebackType | None
    ) -> None: ...
    source_address: tuple[str, int] | None
    def __init__(
        self,
        host: str = "",
        user: str = "",
        passwd: str = "",
        acct: str = "",
        timeout: float | None = ...,
        source_address: tuple[str, int] | None = None,
        *,
        encoding: str = "utf-8",
    ) -> None:
        """Initialization method (called by class instantiation).
        Initialize host to localhost, port to standard ftp port.
        Optional arguments are host (for connect()),
        and user, passwd, acct (for login()).
        """

    def connect(self, host: str = "", port: int = 0, timeout: float = -999, source_address: tuple[str, int] | None = None) -> str:
        """Connect to host.  Arguments are:
        - host: hostname to connect to (string, default previous host)
        - port: port to connect to (integer, default previous port)
        - timeout: the timeout to set against the ftp socket(s)
        - source_address: a 2-tuple (host, port) for the socket to bind
          to as its source address before connecting.
        """

    def getwelcome(self) -> str:
        """Get the welcome message from the server.
        (this is read and squirreled away by connect())
        """

    def set_debuglevel(self, level: int) -> None:
        """Set the debugging level.
        The required argument level means:
        0: no debugging output (default)
        1: print commands and responses but not body text etc.
        2: also print raw lines read and sent before stripping CR/LF
        """

    def debug(self, level: int) -> None:
        """Set the debugging level.
        The required argument level means:
        0: no debugging output (default)
        1: print commands and responses but not body text etc.
        2: also print raw lines read and sent before stripping CR/LF
        """

    def set_pasv(self, val: bool | Literal[0, 1]) -> None:
        """Use passive or active mode for data transfers.
        With a false argument, use the normal PORT mode,
        With a true argument, use the PASV command.
        """

    def sanitize(self, s: str) -> str: ...
    def putline(self, line: str) -> None: ...
    def putcmd(self, line: str) -> None: ...
    def getline(self) -> str: ...
    def getmultiline(self) -> str: ...
    def getresp(self) -> str: ...
    def voidresp(self) -> str:
        """Expect a response beginning with '2'."""

    def abort(self) -> str:
        """Abort a file transfer.  Uses out-of-band data.
        This does not follow the procedure from the RFC to send Telnet
        IP and Synch; that doesn't seem to work with the servers I've
        tried.  Instead, just send the ABOR command as OOB data.
        """

    def sendcmd(self, cmd: str) -> str:
        """Send a command and return the response."""

    def voidcmd(self, cmd: str) -> str:
        """Send a command and expect a response beginning with '2'."""

    def sendport(self, host: str, port: int) -> str:
        """Send a PORT command with the current host and the given
        port number.
        """

    def sendeprt(self, host: str, port: int) -> str:
        """Send an EPRT command with the current host and the given port number."""

    def makeport(self) -> socket:
        """Create a new socket and send a PORT command for it."""

    def makepasv(self) -> tuple[str, int]:
        """Internal: Does the PASV or EPSV handshake -> (address, port)"""

    def login(self, user: str = "", passwd: str = "", acct: str = "") -> str:
        """Login, default anonymous."""
    # In practice, `rest` can actually be anything whose str() is an integer sequence, so to make it simple we allow integers
    def ntransfercmd(self, cmd: str, rest: int | str | None = None) -> tuple[socket, int | None]:
        """Initiate a transfer over the data connection.

        If the transfer is active, send a port command and the
        transfer command, and accept the connection.  If the server is
        passive, send a pasv command, connect to it, and start the
        transfer command.  Either way, return the socket for the
        connection and the expected size of the transfer.  The
        expected size may be None if it could not be determined.

        Optional 'rest' argument can be a string that is sent as the
        argument to a REST command.  This is essentially a server
        marker used to tell the server to skip over any data up to the
        given marker.
        """

    def transfercmd(self, cmd: str, rest: int | str | None = None) -> socket:
        """Like ntransfercmd() but returns only the socket."""

    def retrbinary(
        self, cmd: str, callback: Callable[[bytes], object], blocksize: int = 8192, rest: int | str | None = None
    ) -> str:
        """Retrieve data in binary mode.  A new port is created for you.

        Args:
          cmd: A RETR command.
          callback: A single parameter callable to be called on each
                    block of data read.
          blocksize: The maximum number of bytes to read from the
                     socket at one time.  [default: 8192]
          rest: Passed to transfercmd().  [default: None]

        Returns:
          The response code.
        """

    def storbinary(
        self,
        cmd: str,
        fp: SupportsRead[bytes],
        blocksize: int = 8192,
        callback: Callable[[bytes], object] | None = None,
        rest: int | str | None = None,
    ) -> str:
        """Store a file in binary mode.  A new port is created for you.

        Args:
          cmd: A STOR command.
          fp: A file-like object with a read(num_bytes) method.
          blocksize: The maximum data size to read from fp and send over
                     the connection at once.  [default: 8192]
          callback: An optional single parameter callable that is called on
                    each block of data after it is sent.  [default: None]
          rest: Passed to transfercmd().  [default: None]

        Returns:
          The response code.
        """

    def retrlines(self, cmd: str, callback: Callable[[str], object] | None = None) -> str:
        """Retrieve data in line mode.  A new port is created for you.

        Args:
          cmd: A RETR, LIST, or NLST command.
          callback: An optional single parameter callable that is called
                    for each line with the trailing CRLF stripped.
                    [default: print_line()]

        Returns:
          The response code.
        """

    def storlines(self, cmd: str, fp: SupportsReadline[bytes], callback: Callable[[bytes], object] | None = None) -> str:
        """Store a file in line mode.  A new port is created for you.

        Args:
          cmd: A STOR command.
          fp: A file-like object with a readline() method.
          callback: An optional single parameter callable that is called on
                    each line after it is sent.  [default: None]

        Returns:
          The response code.
        """

    def acct(self, password: str) -> str:
        """Send new account name."""

    def nlst(self, *args: str) -> list[str]:
        """Return a list of files in a given directory (default the current)."""
    # Technically only the last arg can be a Callable but ...
    def dir(self, *args: str | Callable[[str], object]) -> None:
        """List a directory in long form.
        By default list current directory to stdout.
        Optional last argument is callback function; all
        non-empty arguments before it are concatenated to the
        LIST command.  (This *should* only be used for a pathname.)
        """

    def mlsd(self, path: str = "", facts: Iterable[str] = []) -> Iterator[tuple[str, dict[str, str]]]:
        """List a directory in a standardized format by using MLSD
        command (RFC-3659). If path is omitted the current directory
        is assumed. "facts" is a list of strings representing the type
        of information desired (e.g. ["type", "size", "perm"]).

        Return a generator object yielding a tuple of two elements
        for every file found in path.
        First element is the file name, the second one is a dictionary
        including a variable number of "facts" depending on the server
        and whether "facts" argument has been provided.
        """

    def rename(self, fromname: str, toname: str) -> str:
        """Rename a file."""

    def delete(self, filename: str) -> str:
        """Delete a file."""

    def cwd(self, dirname: str) -> str:
        """Change to a directory."""

    def size(self, filename: str) -> int | None:
        """Retrieve the size of a file."""

    def mkd(self, dirname: str) -> str:
        """Make a directory, return its full pathname."""

    def rmd(self, dirname: str) -> str:
        """Remove a directory."""

    def pwd(self) -> str:
        """Return current working directory."""

    def quit(self) -> str:
        """Quit, and close the connection."""

    def close(self) -> None:
        """Close the connection without assuming anything about it."""

class FTP_TLS(FTP):
    """A FTP subclass which adds TLS support to FTP as described
    in RFC-4217.

    Connect as usual to port 21 implicitly securing the FTP control
    connection before authenticating.

    Securing the data connection requires user to explicitly ask
    for it by calling prot_p() method.

    Usage example:
    >>> from ftplib import FTP_TLS
    >>> ftps = FTP_TLS('ftp.python.org')
    >>> ftps.login()  # login anonymously previously securing control channel
    '230 Guest login ok, access restrictions apply.'
    >>> ftps.prot_p()  # switch to secure data connection
    '200 Protection level set to P'
    >>> ftps.retrlines('LIST')  # list directory content securely
    total 9
    drwxr-xr-x   8 root     wheel        1024 Jan  3  1994 .
    drwxr-xr-x   8 root     wheel        1024 Jan  3  1994 ..
    drwxr-xr-x   2 root     wheel        1024 Jan  3  1994 bin
    drwxr-xr-x   2 root     wheel        1024 Jan  3  1994 etc
    d-wxrwxr-x   2 ftp      wheel        1024 Sep  5 13:43 incoming
    drwxr-xr-x   2 root     wheel        1024 Nov 17  1993 lib
    drwxr-xr-x   6 1094     wheel        1024 Sep 13 19:07 pub
    drwxr-xr-x   3 root     wheel        1024 Jan  3  1994 usr
    -rw-r--r--   1 root     root          312 Aug  1  1994 welcome.msg
    '226 Transfer complete.'
    >>> ftps.quit()
    '221 Goodbye.'
    >>>
    """

    if sys.version_info >= (3, 12):
        def __init__(
            self,
            host: str = "",
            user: str = "",
            passwd: str = "",
            acct: str = "",
            *,
            context: SSLContext | None = None,
            timeout: float | None = ...,
            source_address: tuple[str, int] | None = None,
            encoding: str = "utf-8",
        ) -> None: ...
    else:
        def __init__(
            self,
            host: str = "",
            user: str = "",
            passwd: str = "",
            acct: str = "",
            keyfile: str | None = None,
            certfile: str | None = None,
            context: SSLContext | None = None,
            timeout: float | None = ...,
            source_address: tuple[str, int] | None = None,
            *,
            encoding: str = "utf-8",
        ) -> None: ...
    ssl_version: int
    keyfile: str | None
    certfile: str | None
    context: SSLContext
    def login(self, user: str = "", passwd: str = "", acct: str = "", secure: bool = True) -> str: ...
    def auth(self) -> str:
        """Set up secure control connection by using TLS/SSL."""

    def prot_p(self) -> str:
        """Set up secure data connection."""

    def prot_c(self) -> str:
        """Set up clear text data connection."""

    def ccc(self) -> str:
        """Switch back to a clear-text control connection."""

def parse150(resp: str) -> int | None:  # undocumented
    """Parse the '150' response for a RETR request.
    Returns the expected transfer size or None; size is not guaranteed to
    be present in the 150 message.
    """

def parse227(resp: str) -> tuple[str, int]:  # undocumented
    """Parse the '227' response for a PASV request.
    Raises error_proto if it does not contain '(h1,h2,h3,h4,p1,p2)'
    Return ('host.addr.as.numbers', port#) tuple.
    """

def parse229(resp: str, peer: Any) -> tuple[str, int]:  # undocumented
    """Parse the '229' response for an EPSV request.
    Raises error_proto if it does not contain '(|||port|)'
    Return ('host.addr.as.numbers', port#) tuple.
    """

def parse257(resp: str) -> str:  # undocumented
    """Parse the '257' response for a MKD or PWD request.
    This is a response to a MKD or PWD request: a directory name.
    Returns the directoryname in the 257 reply.
    """

def ftpcp(source: FTP, sourcename: str, target: FTP, targetname: str = "", type: Literal["A", "I"] = "I") -> None:  # undocumented
    """Copy file from one FTP-instance to another."""
