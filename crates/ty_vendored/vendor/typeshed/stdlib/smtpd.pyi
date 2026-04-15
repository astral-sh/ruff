"""An RFC 5321 smtp proxy with optional RFC 1870 and RFC 6531 extensions.

Usage: %(program)s [options] [localhost:localport [remotehost:remoteport]]

Options:

    --nosetuid
    -n
        This program generally tries to setuid `nobody', unless this flag is
        set.  The setuid call will fail if this program is not run as root (in
        which case, use this flag).

    --version
    -V
        Print the version number and exit.

    --class classname
    -c classname
        Use `classname' as the concrete SMTP proxy class.  Uses `PureProxy' by
        default.

    --size limit
    -s limit
        Restrict the total size of the incoming message to "limit" number of
        bytes via the RFC 1870 SIZE extension.  Defaults to 33554432 bytes.

    --smtputf8
    -u
        Enable the SMTPUTF8 extension and behave as an RFC 6531 smtp proxy.

    --debug
    -d
        Turn on debugging prints.

    --help
    -h
        Print this message and exit.

Version: %(__version__)s

If localhost is not given then `localhost' is used, and if localport is not
given then 8025 is used.  If remotehost is not given then `localhost' is used,
and if remoteport is not given, then 25 is used.
"""

import asynchat
import asyncore
import socket
import sys
from collections import defaultdict
from typing import Any
from typing_extensions import TypeAlias, deprecated

if sys.version_info >= (3, 11):
    __all__ = ["SMTPChannel", "SMTPServer", "DebuggingServer", "PureProxy"]
else:
    __all__ = ["SMTPChannel", "SMTPServer", "DebuggingServer", "PureProxy", "MailmanProxy"]

_Address: TypeAlias = tuple[str, int]  # (host, port)

class SMTPChannel(asynchat.async_chat):
    COMMAND: int
    DATA: int

    command_size_limits: defaultdict[str, int]
    smtp_server: SMTPServer
    conn: socket.socket
    addr: Any
    received_lines: list[str]
    smtp_state: int
    seen_greeting: str
    mailfrom: str
    rcpttos: list[str]
    received_data: str
    fqdn: str
    peer: str

    command_size_limit: int
    data_size_limit: int

    enable_SMTPUTF8: bool
    @property
    def max_command_size_limit(self) -> int: ...
    def __init__(
        self,
        server: SMTPServer,
        conn: socket.socket,
        addr: Any,
        data_size_limit: int = 33554432,
        map: asyncore._MapType | None = None,
        enable_SMTPUTF8: bool = False,
        decode_data: bool = False,
    ) -> None: ...
    # base asynchat.async_chat.push() accepts bytes
    def push(self, msg: str) -> None: ...  # type: ignore[override]
    def collect_incoming_data(self, data: bytes) -> None: ...
    def found_terminator(self) -> None: ...
    def smtp_HELO(self, arg: str) -> None: ...
    def smtp_NOOP(self, arg: str) -> None: ...
    def smtp_QUIT(self, arg: str) -> None: ...
    def smtp_MAIL(self, arg: str) -> None: ...
    def smtp_RCPT(self, arg: str) -> None: ...
    def smtp_RSET(self, arg: str) -> None: ...
    def smtp_DATA(self, arg: str) -> None: ...
    def smtp_EHLO(self, arg: str) -> None: ...
    def smtp_HELP(self, arg: str) -> None: ...
    def smtp_VRFY(self, arg: str) -> None: ...
    def smtp_EXPN(self, arg: str) -> None: ...

class SMTPServer(asyncore.dispatcher):
    channel_class: type[SMTPChannel]

    data_size_limit: int
    enable_SMTPUTF8: bool
    def __init__(
        self,
        localaddr: _Address,
        remoteaddr: _Address,
        data_size_limit: int = 33554432,
        map: asyncore._MapType | None = None,
        enable_SMTPUTF8: bool = False,
        decode_data: bool = False,
    ) -> None: ...
    def handle_accepted(self, conn: socket.socket, addr: Any) -> None: ...
    def process_message(self, peer: _Address, mailfrom: str, rcpttos: list[str], data: bytes | str, **kwargs: Any) -> str | None:
        """Override this abstract method to handle messages from the client.

        peer is a tuple containing (ipaddr, port) of the client that made the
        socket connection to our smtp port.

        mailfrom is the raw address the client claims the message is coming
        from.

        rcpttos is a list of raw addresses the client wishes to deliver the
        message to.

        data is a string containing the entire full text of the message,
        headers (if supplied) and all.  It has been `de-transparencied'
        according to RFC 821, Section 4.5.2.  In other words, a line
        containing a `.' followed by other text has had the leading dot
        removed.

        kwargs is a dictionary containing additional information.  It is
        empty if decode_data=True was given as init parameter, otherwise
        it will contain the following keys:
            'mail_options': list of parameters to the mail command.  All
                            elements are uppercase strings.  Example:
                            ['BODY=8BITMIME', 'SMTPUTF8'].
            'rcpt_options': same, for the rcpt command.

        This function should return None for a normal `250 Ok' response;
        otherwise, it should return the desired response string in RFC 821
        format.

        """

class DebuggingServer(SMTPServer): ...

class PureProxy(SMTPServer):
    def process_message(self, peer: _Address, mailfrom: str, rcpttos: list[str], data: bytes | str) -> str | None: ...  # type: ignore[override]

if sys.version_info < (3, 11):
    @deprecated("Deprecated since Python 3.9; removed in Python 3.11.")
    class MailmanProxy(PureProxy):
        def process_message(self, peer: _Address, mailfrom: str, rcpttos: list[str], data: bytes | str) -> str | None: ...  # type: ignore[override]
