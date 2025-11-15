"""IMAP4 client.

Based on RFC 2060.

Public class:           IMAP4
Public variable:        Debug
Public functions:       Internaldate2tuple
                        Int2AP
                        ParseFlags
                        Time2Internaldate
"""

import subprocess
import sys
import time
from _typeshed import ReadableBuffer, SizedBuffer, Unused
from builtins import list as _list  # conflicts with a method named "list"
from collections.abc import Callable, Generator
from datetime import datetime
from re import Pattern
from socket import socket as _socket
from ssl import SSLContext, SSLSocket
from types import TracebackType
from typing import IO, Any, Literal, SupportsAbs, SupportsInt
from typing_extensions import Self, TypeAlias, deprecated

__all__ = ["IMAP4", "IMAP4_stream", "Internaldate2tuple", "Int2AP", "ParseFlags", "Time2Internaldate", "IMAP4_SSL"]

# TODO: Commands should use their actual return types, not this type alias.
#       E.g. Tuple[Literal["OK"], List[bytes]]
_CommandResults: TypeAlias = tuple[str, list[Any]]

_AnyResponseData: TypeAlias = list[None] | list[bytes | tuple[bytes, bytes]]

Commands: dict[str, tuple[str, ...]]

class IMAP4:
    """IMAP4 client class.

    Instantiate with: IMAP4([host[, port[, timeout=None]]])

            host - host's name (default: localhost);
            port - port number (default: standard IMAP4 port).
            timeout - socket timeout (default: None)
                      If timeout is not given or is None,
                      the global default socket timeout is used

    All IMAP4rev1 commands are supported by methods of the same
    name (in lowercase).

    All arguments to commands are converted to strings, except for
    AUTHENTICATE, and the last argument to APPEND which is passed as
    an IMAP4 literal.  If necessary (the string contains any
    non-printing characters or white-space and isn't enclosed with
    either parentheses or double quotes) each string is quoted.
    However, the 'password' argument to the LOGIN command is always
    quoted.  If you want to avoid having an argument string quoted
    (eg: the 'flags' argument to STORE) then enclose the string in
    parentheses (eg: "(\\Deleted)").

    Each command returns a tuple: (type, [data, ...]) where 'type'
    is usually 'OK' or 'NO', and 'data' is either the text from the
    tagged response, or untagged results from command. Each 'data'
    is either a string, or a tuple. If a tuple, then the first part
    is the header of the response, and the second part contains
    the data (ie: 'literal' value).

    Errors raise the exception class <instance>.error("<reason>").
    IMAP4 server errors raise <instance>.abort("<reason>"),
    which is a sub-class of 'error'. Mailbox status changes
    from READ-WRITE to READ-ONLY raise the exception class
    <instance>.readonly("<reason>"), which is a sub-class of 'abort'.

    "error" exceptions imply a program error.
    "abort" exceptions imply the connection should be reset, and
            the command re-tried.
    "readonly" exceptions imply the command should be re-tried.

    Note: to use this module, you must read the RFCs pertaining to the
    IMAP4 protocol, as the semantics of the arguments to each IMAP4
    command are left to the invoker, not to mention the results. Also,
    most IMAP servers implement a sub-set of the commands available here.
    """

    class error(Exception): ...
    class abort(error): ...
    class readonly(abort): ...
    utf8_enabled: bool
    mustquote: Pattern[str]
    debug: int
    state: str
    literal: str | None
    tagged_commands: dict[bytes, _list[bytes] | None]
    untagged_responses: dict[str, _list[bytes | tuple[bytes, bytes]]]
    continuation_response: str
    is_readonly: bool
    tagnum: int
    tagpre: str
    tagre: Pattern[str]
    welcome: bytes
    capabilities: tuple[str, ...]
    PROTOCOL_VERSION: str
    def __init__(self, host: str = "", port: int = 143, timeout: float | None = None) -> None: ...
    def open(self, host: str = "", port: int = 143, timeout: float | None = None) -> None:
        """Setup connection to remote server on "host:port"
            (default: localhost:standard IMAP4 port).
        This connection will be used by the routines:
            read, readline, send, shutdown.
        """
    if sys.version_info >= (3, 14):
        @property
        @deprecated("IMAP4.file is unsupported, can cause errors, and may be removed.")
        def file(self) -> IO[str] | IO[bytes]: ...
    else:
        file: IO[str] | IO[bytes]

    def __getattr__(self, attr: str) -> Any: ...
    host: str
    port: int
    sock: _socket
    def read(self, size: int) -> bytes:
        """Read 'size' bytes from remote."""

    def readline(self) -> bytes:
        """Read line from remote."""

    def send(self, data: ReadableBuffer) -> None:
        """Send data to remote."""

    def shutdown(self) -> None:
        """Close I/O established in "open"."""

    def socket(self) -> _socket:
        """Return socket instance used to connect to IMAP4 server.

        socket = <instance>.socket()
        """

    def recent(self) -> _CommandResults:
        """Return most recent 'RECENT' responses if any exist,
        else prompt server for an update using the 'NOOP' command.

        (typ, [data]) = <instance>.recent()

        'data' is None if no new messages,
        else list of RECENT responses, most recent last.
        """

    def response(self, code: str) -> _CommandResults:
        """Return data for response 'code' if received, or None.

        Old value for response 'code' is cleared.

        (code, [data]) = <instance>.response(code)
        """

    def append(self, mailbox: str, flags: str, date_time: str, message: ReadableBuffer) -> str:
        """Append message to named mailbox.

        (typ, [data]) = <instance>.append(mailbox, flags, date_time, message)

                All args except 'message' can be None.
        """

    def authenticate(self, mechanism: str, authobject: Callable[[bytes], bytes | None]) -> tuple[str, str]:
        """Authenticate command - requires response processing.

        'mechanism' specifies which authentication mechanism is to
        be used - it must appear in <instance>.capabilities in the
        form AUTH=<mechanism>.

        'authobject' must be a callable object:

                data = authobject(response)

        It will be called to process server continuation responses; the
        response argument it is passed will be a bytes.  It should return bytes
        data that will be base64 encoded and sent to the server.  It should
        return None if the client abort response '*' should be sent instead.
        """

    def capability(self) -> _CommandResults:
        """(typ, [data]) = <instance>.capability()
        Fetch capabilities list from server.
        """

    def check(self) -> _CommandResults:
        """Checkpoint mailbox on server.

        (typ, [data]) = <instance>.check()
        """

    def close(self) -> _CommandResults:
        """Close currently selected mailbox.

        Deleted messages are removed from writable mailbox.
        This is the recommended command before 'LOGOUT'.

        (typ, [data]) = <instance>.close()
        """

    def copy(self, message_set: str, new_mailbox: str) -> _CommandResults:
        """Copy 'message_set' messages onto end of 'new_mailbox'.

        (typ, [data]) = <instance>.copy(message_set, new_mailbox)
        """

    def create(self, mailbox: str) -> _CommandResults:
        """Create new mailbox.

        (typ, [data]) = <instance>.create(mailbox)
        """

    def delete(self, mailbox: str) -> _CommandResults:
        """Delete old mailbox.

        (typ, [data]) = <instance>.delete(mailbox)
        """

    def deleteacl(self, mailbox: str, who: str) -> _CommandResults:
        """Delete the ACLs (remove any rights) set for who on mailbox.

        (typ, [data]) = <instance>.deleteacl(mailbox, who)
        """

    def enable(self, capability: str) -> _CommandResults:
        """Send an RFC5161 enable string to the server.

        (typ, [data]) = <instance>.enable(capability)
        """

    def __enter__(self) -> Self: ...
    def __exit__(self, t: type[BaseException] | None, v: BaseException | None, tb: TracebackType | None) -> None: ...
    def expunge(self) -> _CommandResults:
        """Permanently remove deleted items from selected mailbox.

        Generates 'EXPUNGE' response for each deleted message.

        (typ, [data]) = <instance>.expunge()

        'data' is list of 'EXPUNGE'd message numbers in order received.
        """

    def fetch(self, message_set: str, message_parts: str) -> tuple[str, _AnyResponseData]:
        """Fetch (parts of) messages.

        (typ, [data, ...]) = <instance>.fetch(message_set, message_parts)

        'message_parts' should be a string of selected parts
        enclosed in parentheses, eg: "(UID BODY[TEXT])".

        'data' are tuples of message part envelope and data.
        """

    def getacl(self, mailbox: str) -> _CommandResults:
        """Get the ACLs for a mailbox.

        (typ, [data]) = <instance>.getacl(mailbox)
        """

    def getannotation(self, mailbox: str, entry: str, attribute: str) -> _CommandResults:
        """(typ, [data]) = <instance>.getannotation(mailbox, entry, attribute)
        Retrieve ANNOTATIONs.
        """

    def getquota(self, root: str) -> _CommandResults:
        """Get the quota root's resource usage and limits.

        Part of the IMAP4 QUOTA extension defined in rfc2087.

        (typ, [data]) = <instance>.getquota(root)
        """

    def getquotaroot(self, mailbox: str) -> _CommandResults:
        """Get the list of quota roots for the named mailbox.

        (typ, [[QUOTAROOT responses...], [QUOTA responses]]) = <instance>.getquotaroot(mailbox)
        """
    if sys.version_info >= (3, 14):
        def idle(self, duration: float | None = None) -> Idler:
            """Return an iterable IDLE context manager producing untagged responses.
            If the argument is not None, limit iteration to 'duration' seconds.

            with M.idle(duration=29 * 60) as idler:
                for typ, data in idler:
                    print(typ, data)

            Note: 'duration' requires a socket connection (not IMAP4_stream).
            """

    def list(self, directory: str = '""', pattern: str = "*") -> tuple[str, _AnyResponseData]:
        """List mailbox names in directory matching pattern.

        (typ, [data]) = <instance>.list(directory='""', pattern='*')

        'data' is list of LIST responses.
        """

    def login(self, user: str, password: str) -> tuple[Literal["OK"], _list[bytes]]:
        """Identify client using plaintext password.

        (typ, [data]) = <instance>.login(user, password)

        NB: 'password' will be quoted.
        """

    def login_cram_md5(self, user: str, password: str) -> _CommandResults:
        """Force use of CRAM-MD5 authentication.

        (typ, [data]) = <instance>.login_cram_md5(user, password)
        """

    def logout(self) -> tuple[str, _AnyResponseData]:
        """Shutdown connection to server.

        (typ, [data]) = <instance>.logout()

        Returns server 'BYE' response.
        """

    def lsub(self, directory: str = '""', pattern: str = "*") -> _CommandResults:
        """List 'subscribed' mailbox names in directory matching pattern.

        (typ, [data, ...]) = <instance>.lsub(directory='""', pattern='*')

        'data' are tuples of message part envelope and data.
        """

    def myrights(self, mailbox: str) -> _CommandResults:
        """Show my ACLs for a mailbox (i.e. the rights that I have on mailbox).

        (typ, [data]) = <instance>.myrights(mailbox)
        """

    def namespace(self) -> _CommandResults:
        """Returns IMAP namespaces ala rfc2342

        (typ, [data, ...]) = <instance>.namespace()
        """

    def noop(self) -> tuple[str, _list[bytes]]:
        """Send NOOP command.

        (typ, [data]) = <instance>.noop()
        """

    def partial(self, message_num: str, message_part: str, start: str, length: str) -> _CommandResults:
        """Fetch truncated part of a message.

        (typ, [data, ...]) = <instance>.partial(message_num, message_part, start, length)

        'data' is tuple of message part envelope and data.
        """

    def proxyauth(self, user: str) -> _CommandResults:
        """Assume authentication as "user".

        Allows an authorised administrator to proxy into any user's
        mailbox.

        (typ, [data]) = <instance>.proxyauth(user)
        """

    def rename(self, oldmailbox: str, newmailbox: str) -> _CommandResults:
        """Rename old mailbox name to new.

        (typ, [data]) = <instance>.rename(oldmailbox, newmailbox)
        """

    def search(self, charset: str | None, *criteria: str) -> _CommandResults:
        """Search mailbox for matching messages.

        (typ, [data]) = <instance>.search(charset, criterion, ...)

        'data' is space separated list of matching message numbers.
        If UTF8 is enabled, charset MUST be None.
        """

    def select(self, mailbox: str = "INBOX", readonly: bool = False) -> tuple[str, _list[bytes | None]]:
        """Select a mailbox.

        Flush all untagged responses.

        (typ, [data]) = <instance>.select(mailbox='INBOX', readonly=False)

        'data' is count of messages in mailbox ('EXISTS' response).

        Mandated responses are ('FLAGS', 'EXISTS', 'RECENT', 'UIDVALIDITY'), so
        other responses should be obtained via <instance>.response('FLAGS') etc.
        """

    def setacl(self, mailbox: str, who: str, what: str) -> _CommandResults:
        """Set a mailbox acl.

        (typ, [data]) = <instance>.setacl(mailbox, who, what)
        """

    def setannotation(self, *args: str) -> _CommandResults:
        """(typ, [data]) = <instance>.setannotation(mailbox[, entry, attribute]+)
        Set ANNOTATIONs.
        """

    def setquota(self, root: str, limits: str) -> _CommandResults:
        """Set the quota root's resource limits.

        (typ, [data]) = <instance>.setquota(root, limits)
        """

    def sort(self, sort_criteria: str, charset: str, *search_criteria: str) -> _CommandResults:
        """IMAP4rev1 extension SORT command.

        (typ, [data]) = <instance>.sort(sort_criteria, charset, search_criteria, ...)
        """

    def starttls(self, ssl_context: Any | None = None) -> tuple[Literal["OK"], _list[None]]: ...
    def status(self, mailbox: str, names: str) -> _CommandResults:
        """Request named status conditions for mailbox.

        (typ, [data]) = <instance>.status(mailbox, names)
        """

    def store(self, message_set: str, command: str, flags: str) -> _CommandResults:
        """Alters flag dispositions for messages in mailbox.

        (typ, [data]) = <instance>.store(message_set, command, flags)
        """

    def subscribe(self, mailbox: str) -> _CommandResults:
        """Subscribe to new mailbox.

        (typ, [data]) = <instance>.subscribe(mailbox)
        """

    def thread(self, threading_algorithm: str, charset: str, *search_criteria: str) -> _CommandResults:
        """IMAPrev1 extension THREAD command.

        (type, [data]) = <instance>.thread(threading_algorithm, charset, search_criteria, ...)
        """

    def uid(self, command: str, *args: str) -> _CommandResults:
        """Execute "command arg ..." with messages identified by UID,
                rather than message number.

        (typ, [data]) = <instance>.uid(command, arg1, arg2, ...)

        Returns response appropriate to 'command'.
        """

    def unsubscribe(self, mailbox: str) -> _CommandResults:
        """Unsubscribe from old mailbox.

        (typ, [data]) = <instance>.unsubscribe(mailbox)
        """

    def unselect(self) -> _CommandResults:
        """Free server's resources associated with the selected mailbox
        and returns the server to the authenticated state.
        This command performs the same actions as CLOSE, except
        that no messages are permanently removed from the currently
        selected mailbox.

        (typ, [data]) = <instance>.unselect()
        """

    def xatom(self, name: str, *args: str) -> _CommandResults:
        """Allow simple extension commands
                notified by server in CAPABILITY response.

        Assumes command is legal in current state.

        (typ, [data]) = <instance>.xatom(name, arg, ...)

        Returns response appropriate to extension command 'name'.
        """

    def print_log(self) -> None: ...

if sys.version_info >= (3, 14):
    class Idler:
        """Iterable IDLE context manager: start IDLE & produce untagged responses.

        An object of this type is returned by the IMAP4.idle() method.

        Note: The name and structure of this class are subject to change.
        """

        def __init__(self, imap: IMAP4, duration: float | None = None) -> None: ...
        def __enter__(self) -> Self: ...
        def __exit__(self, exc_type: object, exc_val: Unused, exc_tb: Unused) -> Literal[False]: ...
        def __iter__(self) -> Self: ...
        def __next__(self) -> tuple[str, float | None]: ...
        def burst(self, interval: float = 0.1) -> Generator[tuple[str, float | None]]:
            """Yield a burst of responses no more than 'interval' seconds apart.

            with M.idle() as idler:
                # get a response and any others following by < 0.1 seconds
                batch = list(idler.burst())
                print(f'processing {len(batch)} responses...')
                print(batch)

            Note: This generator requires a socket connection (not IMAP4_stream).
            """

class IMAP4_SSL(IMAP4):
    """IMAP4 client class over SSL connection

    Instantiate with: IMAP4_SSL([host[, port[, ssl_context[, timeout=None]]]])

            host - host's name (default: localhost);
            port - port number (default: standard IMAP4 SSL port);
            ssl_context - a SSLContext object that contains your certificate chain
                          and private key (default: None)
            timeout - socket timeout (default: None) If timeout is not given or is None,
                      the global default socket timeout is used

    for more documentation see the docstring of the parent class IMAP4.
    """

    if sys.version_info < (3, 12):
        keyfile: str
        certfile: str
    if sys.version_info >= (3, 12):
        def __init__(
            self, host: str = "", port: int = 993, *, ssl_context: SSLContext | None = None, timeout: float | None = None
        ) -> None: ...
    else:
        def __init__(
            self,
            host: str = "",
            port: int = 993,
            keyfile: str | None = None,
            certfile: str | None = None,
            ssl_context: SSLContext | None = None,
            timeout: float | None = None,
        ) -> None: ...
    sslobj: SSLSocket
    if sys.version_info >= (3, 14):
        @property
        @deprecated("IMAP4_SSL.file is unsupported, can cause errors, and may be removed.")
        def file(self) -> IO[Any]: ...
    else:
        file: IO[Any]

    def open(self, host: str = "", port: int | None = 993, timeout: float | None = None) -> None:
        """Setup connection to remote server on "host:port".
            (default: localhost:standard IMAP4 SSL port).
        This connection will be used by the routines:
            read, readline, send, shutdown.
        """

    def ssl(self) -> SSLSocket: ...

class IMAP4_stream(IMAP4):
    """IMAP4 client class over a stream

    Instantiate with: IMAP4_stream(command)

            "command" - a string that can be passed to subprocess.Popen()

    for more documentation see the docstring of the parent class IMAP4.
    """

    command: str
    def __init__(self, command: str) -> None: ...
    if sys.version_info >= (3, 14):
        @property
        @deprecated("IMAP4_stream.file is unsupported, can cause errors, and may be removed.")
        def file(self) -> IO[Any]: ...
    else:
        file: IO[Any]
    process: subprocess.Popen[bytes]
    writefile: IO[Any]
    readfile: IO[Any]
    def open(self, host: str | None = None, port: int | None = None, timeout: float | None = None) -> None:
        """Setup a stream connection.
        This connection will be used by the routines:
            read, readline, send, shutdown.
        """

class _Authenticator:
    """Private class to provide en/decoding
    for base64-based authentication conversation.
    """

    mech: Callable[[bytes], bytes | bytearray | memoryview | str | None]
    def __init__(self, mechinst: Callable[[bytes], bytes | bytearray | memoryview | str | None]) -> None: ...
    def process(self, data: str) -> str: ...
    def encode(self, inp: bytes | bytearray | memoryview) -> str: ...
    def decode(self, inp: str | SizedBuffer) -> bytes: ...

def Internaldate2tuple(resp: ReadableBuffer) -> time.struct_time | None:
    """Parse an IMAP4 INTERNALDATE string.

    Return corresponding local time.  The return value is a
    time.struct_time tuple or None if the string has wrong format.
    """

def Int2AP(num: SupportsAbs[SupportsInt]) -> bytes:
    """Convert integer to A-P string representation."""

def ParseFlags(resp: ReadableBuffer) -> tuple[bytes, ...]:
    """Convert IMAP4 flags response to python tuple."""

def Time2Internaldate(date_time: float | time.struct_time | time._TimeTuple | datetime | str) -> str:
    """Convert date_time to IMAP4 INTERNALDATE representation.

    Return string in form: '"DD-Mmm-YYYY HH:MM:SS +HHMM"'.  The
    date_time argument can be a number (int or float) representing
    seconds since epoch (as returned by time.time()), a 9-tuple
    representing local time, an instance of time.struct_time (as
    returned by time.localtime()), an aware datetime instance or a
    double-quoted string.  In the last case, it is assumed to already
    be in the correct format.
    """
