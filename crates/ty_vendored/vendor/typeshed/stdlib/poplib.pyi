"""A POP3 client class.

Based on the J. Myers POP3 draft, Jan. 96
"""

import socket
import ssl
import sys
from builtins import list as _list  # conflicts with a method named "list"
from re import Pattern
from typing import Any, BinaryIO, Final, NoReturn, overload
from typing_extensions import TypeAlias

__all__ = ["POP3", "error_proto", "POP3_SSL"]

_LongResp: TypeAlias = tuple[bytes, list[bytes], int]

class error_proto(Exception): ...

POP3_PORT: Final = 110
POP3_SSL_PORT: Final = 995
CR: Final = b"\r"
LF: Final = b"\n"
CRLF: Final = b"\r\n"
HAVE_SSL: Final[bool]

class POP3:
    """This class supports both the minimal and optional command sets.
    Arguments can be strings or integers (where appropriate)
    (e.g.: retr(1) and retr('1') both work equally well.

    Minimal Command Set:
            USER name               user(name)
            PASS string             pass_(string)
            STAT                    stat()
            LIST [msg]              list(msg = None)
            RETR msg                retr(msg)
            DELE msg                dele(msg)
            NOOP                    noop()
            RSET                    rset()
            QUIT                    quit()

    Optional Commands (some servers support these):
            RPOP name               rpop(name)
            APOP name digest        apop(name, digest)
            TOP msg n               top(msg, n)
            UIDL [msg]              uidl(msg = None)
            CAPA                    capa()
            STLS                    stls()
            UTF8                    utf8()

    Raises one exception: 'error_proto'.

    Instantiate with:
            POP3(hostname, port=110)

    NB:     the POP protocol locks the mailbox from user
            authorization until QUIT, so be sure to get in, suck
            the messages, and quit, each time you access the
            mailbox.

            POP is a line-based protocol, which means large mail
            messages consume lots of python cycles reading them
            line-by-line.

            If it's available on your mail server, use IMAP4
            instead, it doesn't suffer from the two problems
            above.
    """

    encoding: str
    host: str
    port: int
    sock: socket.socket
    file: BinaryIO
    welcome: bytes
    def __init__(self, host: str, port: int = 110, timeout: float = ...) -> None: ...
    def getwelcome(self) -> bytes: ...
    def set_debuglevel(self, level: int) -> None: ...
    def user(self, user: str) -> bytes:
        """Send user name, return response

        (should indicate password required).
        """

    def pass_(self, pswd: str) -> bytes:
        """Send password, return response

        (response includes message count, mailbox size).

        NB: mailbox is locked by server from here to 'quit()'
        """

    def stat(self) -> tuple[int, int]:
        """Get mailbox status.

        Result is tuple of 2 ints (message count, mailbox size)
        """

    def list(self, which: Any | None = None) -> _LongResp:
        """Request listing, return result.

        Result without a message number argument is in form
        ['response', ['mesg_num octets', ...], octets].

        Result when a message number argument is given is a
        single response: the "scan listing" for that message.
        """

    def retr(self, which: Any) -> _LongResp:
        """Retrieve whole message number 'which'.

        Result is in form ['response', ['line', ...], octets].
        """

    def dele(self, which: Any) -> bytes:
        """Delete message number 'which'.

        Result is 'response'.
        """

    def noop(self) -> bytes:
        """Does nothing.

        One supposes the response indicates the server is alive.
        """

    def rset(self) -> bytes:
        """Unmark all messages marked for deletion."""

    def quit(self) -> bytes:
        """Signoff: commit changes on server, unlock mailbox, close connection."""

    def close(self) -> None:
        """Close the connection without assuming anything about it."""

    def rpop(self, user: str) -> bytes:
        """Send RPOP command to access the mailbox with an alternate user."""
    timestamp: Pattern[str]
    def apop(self, user: str, password: str) -> bytes:
        """Authorisation

        - only possible if server has supplied a timestamp in initial greeting.

        Args:
                user     - mailbox user;
                password - mailbox password.

        NB: mailbox is locked by server from here to 'quit()'
        """

    def top(self, which: Any, howmuch: int) -> _LongResp:
        """Retrieve message header of message number 'which'
        and first 'howmuch' lines of message body.

        Result is in form ['response', ['line', ...], octets].
        """

    @overload
    def uidl(self) -> _LongResp:
        """Return message digest (unique id) list.

        If 'which', result contains unique id for that message
        in the form 'response mesgnum uid', otherwise result is
        the list ['response', ['mesgnum uid', ...], octets]
        """

    @overload
    def uidl(self, which: Any) -> bytes: ...
    def utf8(self) -> bytes:
        """Try to enter UTF-8 mode (see RFC 6856). Returns server response."""

    def capa(self) -> dict[str, _list[str]]:
        """Return server capabilities (RFC 2449) as a dictionary
        >>> c=poplib.POP3('localhost')
        >>> c.capa()
        {'IMPLEMENTATION': ['Cyrus', 'POP3', 'server', 'v2.2.12'],
         'TOP': [], 'LOGIN-DELAY': ['0'], 'AUTH-RESP-CODE': [],
         'EXPIRE': ['NEVER'], 'USER': [], 'STLS': [], 'PIPELINING': [],
         'UIDL': [], 'RESP-CODES': []}
        >>>

        Really, according to RFC 2449, the cyrus folks should avoid
        having the implementation split into multiple arguments...
        """

    def stls(self, context: ssl.SSLContext | None = None) -> bytes:
        """Start a TLS session on the active connection as specified in RFC 2595.

        context - a ssl.SSLContext
        """

class POP3_SSL(POP3):
    """POP3 client class over SSL connection

    Instantiate with: POP3_SSL(hostname, port=995, context=None)

           hostname - the hostname of the pop3 over ssl server
           port - port number
           context - a ssl.SSLContext

    See the methods of the parent class POP3 for more documentation.
    """

    if sys.version_info >= (3, 12):
        def __init__(
            self, host: str, port: int = 995, *, timeout: float = ..., context: ssl.SSLContext | None = None
        ) -> None: ...
        def stls(self, context: Any = None) -> NoReturn:
            """The method unconditionally raises an exception since the
            STLS command doesn't make any sense on an already established
            SSL/TLS session.
            """
    else:
        def __init__(
            self,
            host: str,
            port: int = 995,
            keyfile: str | None = None,
            certfile: str | None = None,
            timeout: float = ...,
            context: ssl.SSLContext | None = None,
        ) -> None: ...
        # "context" is actually the last argument,
        # but that breaks LSP and it doesn't really matter because all the arguments are ignored
        def stls(self, context: Any = None, keyfile: Any = None, certfile: Any = None) -> NoReturn:
            """The method unconditionally raises an exception since the
            STLS command doesn't make any sense on an already established
            SSL/TLS session.
            """
