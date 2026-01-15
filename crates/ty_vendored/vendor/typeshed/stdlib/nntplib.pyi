"""An NNTP client class based on:
- RFC 977: Network News Transfer Protocol
- RFC 2980: Common NNTP Extensions
- RFC 3977: Network News Transfer Protocol (version 2)

Example:

>>> from nntplib import NNTP
>>> s = NNTP('news')
>>> resp, count, first, last, name = s.group('comp.lang.python')
>>> print('Group', name, 'has', count, 'articles, range', first, 'to', last)
Group comp.lang.python has 51 articles, range 5770 to 5821
>>> resp, subs = s.xhdr('subject', '{0}-{1}'.format(first, last))
>>> resp = s.quit()
>>>

Here 'resp' is the server response line.
Error responses are turned into exceptions.

To post an article from a file:
>>> f = open(filename, 'rb') # file containing article, including header
>>> resp = s.post(f)
>>>

For descriptions of all methods, read the comments in the code below.
Note that all arguments and return values representing article numbers
are strings, not numbers, since they are rarely used for calculations.
"""

import datetime
import socket
import ssl
from _typeshed import Unused
from builtins import list as _list  # conflicts with a method named "list"
from collections.abc import Iterable
from typing import IO, Any, Final, NamedTuple
from typing_extensions import Self, TypeAlias

__all__ = [
    "NNTP",
    "NNTPError",
    "NNTPReplyError",
    "NNTPTemporaryError",
    "NNTPPermanentError",
    "NNTPProtocolError",
    "NNTPDataError",
    "decode_header",
    "NNTP_SSL",
]

_File: TypeAlias = IO[bytes] | bytes | str | None

class NNTPError(Exception):
    """Base class for all nntplib exceptions"""

    response: str

class NNTPReplyError(NNTPError):
    """Unexpected [123]xx reply"""

class NNTPTemporaryError(NNTPError):
    """4xx errors"""

class NNTPPermanentError(NNTPError):
    """5xx errors"""

class NNTPProtocolError(NNTPError):
    """Response does not begin with [1-5]"""

class NNTPDataError(NNTPError):
    """Error in response data"""

NNTP_PORT: Final = 119
NNTP_SSL_PORT: Final = 563

class GroupInfo(NamedTuple):
    """GroupInfo(group, last, first, flag)"""

    group: str
    last: str
    first: str
    flag: str

class ArticleInfo(NamedTuple):
    """ArticleInfo(number, message_id, lines)"""

    number: int
    message_id: str
    lines: list[bytes]

def decode_header(header_str: str) -> str:
    """Takes a unicode string representing a munged header value
    and decodes it as a (possibly non-ASCII) readable value.
    """

class NNTP:
    encoding: str
    errors: str

    host: str
    port: int
    sock: socket.socket
    file: IO[bytes]
    debugging: int
    welcome: str
    readermode_afterauth: bool
    tls_on: bool
    authenticated: bool
    nntp_implementation: str
    nntp_version: int
    def __init__(
        self,
        host: str,
        port: int = 119,
        user: str | None = None,
        password: str | None = None,
        readermode: bool | None = None,
        usenetrc: bool = False,
        timeout: float = ...,
    ) -> None:
        """Initialize an instance.  Arguments:
        - host: hostname to connect to
        - port: port to connect to (default the standard NNTP port)
        - user: username to authenticate with
        - password: password to use with username
        - readermode: if true, send 'mode reader' command after
                      connecting.
        - usenetrc: allow loading username and password from ~/.netrc file
                    if not specified explicitly
        - timeout: timeout (in seconds) used for socket connections

        readermode is sometimes necessary if you are connecting to an
        NNTP server on the local machine and intend to call
        reader-specific commands, such as `group'.  If you get
        unexpected NNTPPermanentErrors, you might need to set
        readermode.
        """

    def __enter__(self) -> Self: ...
    def __exit__(self, *args: Unused) -> None: ...
    def getwelcome(self) -> str:
        """Get the welcome message from the server
        (this is read and squirreled away by __init__()).
        If the response code is 200, posting is allowed;
        if it 201, posting is not allowed.
        """

    def getcapabilities(self) -> dict[str, _list[str]]:
        """Get the server capabilities, as read by __init__().
        If the CAPABILITIES command is not supported, an empty dict is
        returned.
        """

    def set_debuglevel(self, level: int) -> None:
        """Set the debugging level.  Argument 'level' means:
        0: no debugging output (default)
        1: print commands and responses but not body text etc.
        2: also print raw lines read and sent before stripping CR/LF
        """

    def debug(self, level: int) -> None:
        """Set the debugging level.  Argument 'level' means:
        0: no debugging output (default)
        1: print commands and responses but not body text etc.
        2: also print raw lines read and sent before stripping CR/LF
        """

    def capabilities(self) -> tuple[str, dict[str, _list[str]]]:
        """Process a CAPABILITIES command.  Not supported by all servers.
        Return:
        - resp: server response if successful
        - caps: a dictionary mapping capability names to lists of tokens
        (for example {'VERSION': ['2'], 'OVER': [], LIST: ['ACTIVE', 'HEADERS'] })
        """

    def newgroups(self, date: datetime.date | datetime.datetime, *, file: _File = None) -> tuple[str, _list[str]]:
        """Process a NEWGROUPS command.  Arguments:
        - date: a date or datetime object
        Return:
        - resp: server response if successful
        - list: list of newsgroup names
        """

    def newnews(self, group: str, date: datetime.date | datetime.datetime, *, file: _File = None) -> tuple[str, _list[str]]:
        """Process a NEWNEWS command.  Arguments:
        - group: group name or '*'
        - date: a date or datetime object
        Return:
        - resp: server response if successful
        - list: list of message ids
        """

    def list(self, group_pattern: str | None = None, *, file: _File = None) -> tuple[str, _list[str]]:
        """Process a LIST or LIST ACTIVE command. Arguments:
        - group_pattern: a pattern indicating which groups to query
        - file: Filename string or file object to store the result in
        Returns:
        - resp: server response if successful
        - list: list of (group, last, first, flag) (strings)
        """

    def description(self, group: str) -> str:
        """Get a description for a single group.  If more than one
        group matches ('group' is a pattern), return the first.  If no
        group matches, return an empty string.

        This elides the response code from the server, since it can
        only be '215' or '285' (for xgtitle) anyway.  If the response
        code is needed, use the 'descriptions' method.

        NOTE: This neither checks for a wildcard in 'group' nor does
        it check whether the group actually exists.
        """

    def descriptions(self, group_pattern: str) -> tuple[str, dict[str, str]]:
        """Get descriptions for a range of groups."""

    def group(self, name: str) -> tuple[str, int, int, int, str]:
        """Process a GROUP command.  Argument:
        - group: the group name
        Returns:
        - resp: server response if successful
        - count: number of articles
        - first: first article number
        - last: last article number
        - name: the group name
        """

    def help(self, *, file: _File = None) -> tuple[str, _list[str]]:
        """Process a HELP command. Argument:
        - file: Filename string or file object to store the result in
        Returns:
        - resp: server response if successful
        - list: list of strings returned by the server in response to the
                HELP command
        """

    def stat(self, message_spec: Any = None) -> tuple[str, int, str]:
        """Process a STAT command.  Argument:
        - message_spec: article number or message id (if not specified,
          the current article is selected)
        Returns:
        - resp: server response if successful
        - art_num: the article number
        - message_id: the message id
        """

    def next(self) -> tuple[str, int, str]:
        """Process a NEXT command.  No arguments.  Return as for STAT."""

    def last(self) -> tuple[str, int, str]:
        """Process a LAST command.  No arguments.  Return as for STAT."""

    def head(self, message_spec: Any = None, *, file: _File = None) -> tuple[str, ArticleInfo]:
        """Process a HEAD command.  Argument:
        - message_spec: article number or message id
        - file: filename string or file object to store the headers in
        Returns:
        - resp: server response if successful
        - ArticleInfo: (article number, message id, list of header lines)
        """

    def body(self, message_spec: Any = None, *, file: _File = None) -> tuple[str, ArticleInfo]:
        """Process a BODY command.  Argument:
        - message_spec: article number or message id
        - file: filename string or file object to store the body in
        Returns:
        - resp: server response if successful
        - ArticleInfo: (article number, message id, list of body lines)
        """

    def article(self, message_spec: Any = None, *, file: _File = None) -> tuple[str, ArticleInfo]:
        """Process an ARTICLE command.  Argument:
        - message_spec: article number or message id
        - file: filename string or file object to store the article in
        Returns:
        - resp: server response if successful
        - ArticleInfo: (article number, message id, list of article lines)
        """

    def slave(self) -> str:
        """Process a SLAVE command.  Returns:
        - resp: server response if successful
        """

    def xhdr(self, hdr: str, str: Any, *, file: _File = None) -> tuple[str, _list[str]]:
        """Process an XHDR command (optional server extension).  Arguments:
        - hdr: the header type (e.g. 'subject')
        - str: an article nr, a message id, or a range nr1-nr2
        - file: Filename string or file object to store the result in
        Returns:
        - resp: server response if successful
        - list: list of (nr, value) strings
        """

    def xover(self, start: int, end: int, *, file: _File = None) -> tuple[str, _list[tuple[int, dict[str, str]]]]:
        """Process an XOVER command (optional server extension) Arguments:
        - start: start of range
        - end: end of range
        - file: Filename string or file object to store the result in
        Returns:
        - resp: server response if successful
        - list: list of dicts containing the response fields
        """

    def over(
        self, message_spec: None | str | _list[Any] | tuple[Any, ...], *, file: _File = None
    ) -> tuple[str, _list[tuple[int, dict[str, str]]]]:
        """Process an OVER command.  If the command isn't supported, fall
        back to XOVER. Arguments:
        - message_spec:
            - either a message id, indicating the article to fetch
              information about
            - or a (start, end) tuple, indicating a range of article numbers;
              if end is None, information up to the newest message will be
              retrieved
            - or None, indicating the current article number must be used
        - file: Filename string or file object to store the result in
        Returns:
        - resp: server response if successful
        - list: list of dicts containing the response fields

        NOTE: the "message id" form isn't supported by XOVER
        """

    def date(self) -> tuple[str, datetime.datetime]:
        """Process the DATE command.
        Returns:
        - resp: server response if successful
        - date: datetime object
        """

    def post(self, data: bytes | Iterable[bytes]) -> str:
        """Process a POST command.  Arguments:
        - data: bytes object, iterable or file containing the article
        Returns:
        - resp: server response if successful
        """

    def ihave(self, message_id: Any, data: bytes | Iterable[bytes]) -> str:
        """Process an IHAVE command.  Arguments:
        - message_id: message-id of the article
        - data: file containing the article
        Returns:
        - resp: server response if successful
        Note that if the server refuses the article an exception is raised.
        """

    def quit(self) -> str:
        """Process a QUIT command and close the socket.  Returns:
        - resp: server response if successful
        """

    def login(self, user: str | None = None, password: str | None = None, usenetrc: bool = True) -> None: ...
    def starttls(self, context: ssl.SSLContext | None = None) -> None:
        """Process a STARTTLS command. Arguments:
        - context: SSL context to use for the encrypted connection
        """

class NNTP_SSL(NNTP):
    ssl_context: ssl.SSLContext | None
    sock: ssl.SSLSocket
    def __init__(
        self,
        host: str,
        port: int = 563,
        user: str | None = None,
        password: str | None = None,
        ssl_context: ssl.SSLContext | None = None,
        readermode: bool | None = None,
        usenetrc: bool = False,
        timeout: float = ...,
    ) -> None:
        """This works identically to NNTP.__init__, except for the change
        in default port and the `ssl_context` argument for SSL connections.
        """
