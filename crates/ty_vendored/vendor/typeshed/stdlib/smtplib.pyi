"""SMTP/ESMTP client class.

This should follow RFC 821 (SMTP), RFC 1869 (ESMTP), RFC 2554 (SMTP
Authentication) and RFC 2487 (Secure SMTP over TLS).

Notes:

Please remember, when doing ESMTP, that the names of the SMTP service
extensions are NOT the same thing as the option keywords for the RCPT
and MAIL commands!

Example:

  >>> import smtplib
  >>> s=smtplib.SMTP("localhost")
  >>> print(s.help())
  This is Sendmail version 8.8.4
  Topics:
      HELO    EHLO    MAIL    RCPT    DATA
      RSET    NOOP    QUIT    HELP    VRFY
      EXPN    VERB    ETRN    DSN
  For more info use "HELP <topic>".
  To report bugs in the implementation send email to
      sendmail-bugs@sendmail.org.
  For local information send email to Postmaster at your site.
  End of HELP info
  >>> s.putcmd("vrfy","someone@here")
  >>> s.getreply()
  (250, "Somebody OverHere <somebody@here.my.org>")
  >>> s.quit()
"""

import sys
from _socket import _Address as _SourceAddress
from _typeshed import ReadableBuffer, SizedBuffer
from collections.abc import Sequence
from email.message import Message as _Message
from re import Pattern
from socket import socket
from ssl import SSLContext
from types import TracebackType
from typing import Any, Final, Protocol, overload, type_check_only
from typing_extensions import Self, TypeAlias

__all__ = [
    "SMTPException",
    "SMTPServerDisconnected",
    "SMTPResponseException",
    "SMTPSenderRefused",
    "SMTPRecipientsRefused",
    "SMTPDataError",
    "SMTPConnectError",
    "SMTPHeloError",
    "SMTPAuthenticationError",
    "quoteaddr",
    "quotedata",
    "SMTP",
    "SMTP_SSL",
    "SMTPNotSupportedError",
]

_Reply: TypeAlias = tuple[int, bytes]
_SendErrs: TypeAlias = dict[str, _Reply]

SMTP_PORT: Final = 25
SMTP_SSL_PORT: Final = 465
CRLF: Final[str]
bCRLF: Final[bytes]

OLDSTYLE_AUTH: Final[Pattern[str]]

class SMTPException(OSError):
    """Base class for all exceptions raised by this module."""

class SMTPNotSupportedError(SMTPException):
    """The command or option is not supported by the SMTP server.

    This exception is raised when an attempt is made to run a command or a
    command with an option which is not supported by the server.
    """

class SMTPServerDisconnected(SMTPException):
    """Not connected to any SMTP server.

    This exception is raised when the server unexpectedly disconnects,
    or when an attempt is made to use the SMTP instance before
    connecting it to a server.
    """

class SMTPResponseException(SMTPException):
    """Base class for all exceptions that include an SMTP error code.

    These exceptions are generated in some instances when the SMTP
    server returns an error code.  The error code is stored in the
    `smtp_code' attribute of the error, and the `smtp_error' attribute
    is set to the error message.
    """

    smtp_code: int
    smtp_error: bytes | str
    args: tuple[int, bytes | str] | tuple[int, bytes, str]
    def __init__(self, code: int, msg: bytes | str) -> None: ...

class SMTPSenderRefused(SMTPResponseException):
    """Sender address refused.

    In addition to the attributes set by on all SMTPResponseException
    exceptions, this sets 'sender' to the string that the SMTP refused.
    """

    smtp_error: bytes
    sender: str
    args: tuple[int, bytes, str]
    def __init__(self, code: int, msg: bytes, sender: str) -> None: ...

class SMTPRecipientsRefused(SMTPException):
    """All recipient addresses refused.

    The errors for each recipient are accessible through the attribute
    'recipients', which is a dictionary of exactly the same sort as
    SMTP.sendmail() returns.
    """

    recipients: _SendErrs
    args: tuple[_SendErrs]
    def __init__(self, recipients: _SendErrs) -> None: ...

class SMTPDataError(SMTPResponseException):
    """The SMTP server didn't accept the data."""

class SMTPConnectError(SMTPResponseException):
    """Error during connection establishment."""

class SMTPHeloError(SMTPResponseException):
    """The server refused our HELO reply."""

class SMTPAuthenticationError(SMTPResponseException):
    """Authentication error.

    Most probably the server didn't accept the username/password
    combination provided.
    """

def quoteaddr(addrstring: str) -> str:
    """Quote a subset of the email addresses defined by RFC 821.

    Should be able to handle anything email.utils.parseaddr can handle.
    """

def quotedata(data: str) -> str:
    """Quote data for email.

    Double leading '.', and change Unix newline '\\n', or Mac '\\r' into
    internet CRLF end-of-line.
    """

@type_check_only
class _AuthObject(Protocol):
    @overload
    def __call__(self, challenge: None = None, /) -> str | None: ...
    @overload
    def __call__(self, challenge: bytes, /) -> str: ...

class SMTP:
    """This class manages a connection to an SMTP or ESMTP server.
    SMTP Objects:
        SMTP objects have the following attributes:
            helo_resp
                This is the message given by the server in response to the
                most recent HELO command.

            ehlo_resp
                This is the message given by the server in response to the
                most recent EHLO command. This is usually multiline.

            does_esmtp
                This is a True value _after you do an EHLO command_, if the
                server supports ESMTP.

            esmtp_features
                This is a dictionary, which, if the server supports ESMTP,
                will _after you do an EHLO command_, contain the names of the
                SMTP service extensions this server supports, and their
                parameters (if any).

                Note, all extension names are mapped to lower case in the
                dictionary.

        See each method's docstrings for details.  In general, there is a
        method of the same name to perform each SMTP command.  There is also a
        method called 'sendmail' that will do an entire mail transaction.
    """

    debuglevel: int
    sock: socket | None
    # Type of file should match what socket.makefile() returns
    file: Any | None
    helo_resp: bytes | None
    ehlo_msg: str
    ehlo_resp: bytes | None
    does_esmtp: bool
    default_port: int
    timeout: float
    esmtp_features: dict[str, str]
    command_encoding: str
    source_address: _SourceAddress | None
    local_hostname: str
    def __init__(
        self,
        host: str = "",
        port: int = 0,
        local_hostname: str | None = None,
        timeout: float = ...,
        source_address: _SourceAddress | None = None,
    ) -> None:
        """Initialize a new instance.

        If specified, `host` is the name of the remote host to which to
        connect.  If specified, `port` specifies the port to which to connect.
        By default, smtplib.SMTP_PORT is used.  If a host is specified the
        connect method is called, and if it returns anything other than a
        success code an SMTPConnectError is raised.  If specified,
        `local_hostname` is used as the FQDN of the local host in the HELO/EHLO
        command.  Otherwise, the local hostname is found using
        socket.getfqdn(). The `source_address` parameter takes a 2-tuple (host,
        port) for the socket to bind to as its source address before
        connecting. If the host is '' and port is 0, the OS default behavior
        will be used.

        """

    def __enter__(self) -> Self: ...
    def __exit__(
        self, exc_type: type[BaseException] | None, exc_value: BaseException | None, tb: TracebackType | None
    ) -> None: ...
    def set_debuglevel(self, debuglevel: int) -> None:
        """Set the debug output level.

        A non-false value results in debug messages for connection and for all
        messages sent to and received from the server.

        """

    def connect(self, host: str = "localhost", port: int = 0, source_address: _SourceAddress | None = None) -> _Reply:
        """Connect to a host on a given port.

        If the hostname ends with a colon (':') followed by a number, and
        there is no port specified, that suffix will be stripped off and the
        number interpreted as the port number to use.

        Note: This method is automatically invoked by __init__, if a host is
        specified during instantiation.

        """

    def send(self, s: ReadableBuffer | str) -> None:
        """Send 's' to the server."""

    def putcmd(self, cmd: str, args: str = "") -> None:
        """Send a command to the server."""

    def getreply(self) -> _Reply:
        """Get a reply from the server.

        Returns a tuple consisting of:

          - server response code (e.g. '250', or such, if all goes well)
            Note: returns -1 if it can't read response code.

          - server response string corresponding to response code (multiline
            responses are converted to a single, multiline string).

        Raises SMTPServerDisconnected if end-of-file is reached.
        """

    def docmd(self, cmd: str, args: str = "") -> _Reply:
        """Send a command, and return its response code."""

    def helo(self, name: str = "") -> _Reply:
        """SMTP 'helo' command.
        Hostname to send for this command defaults to the FQDN of the local
        host.
        """

    def ehlo(self, name: str = "") -> _Reply:
        """SMTP 'ehlo' command.
        Hostname to send for this command defaults to the FQDN of the local
        host.
        """

    def has_extn(self, opt: str) -> bool:
        """Does the server support a given SMTP service extension?"""

    def help(self, args: str = "") -> bytes:
        """SMTP 'help' command.
        Returns help text from server.
        """

    def rset(self) -> _Reply:
        """SMTP 'rset' command -- resets session."""

    def noop(self) -> _Reply:
        """SMTP 'noop' command -- doesn't do anything :>"""

    def mail(self, sender: str, options: Sequence[str] = ()) -> _Reply:
        """SMTP 'mail' command -- begins mail xfer session.

        This method may raise the following exceptions:

         SMTPNotSupportedError  The options parameter includes 'SMTPUTF8'
                                but the SMTPUTF8 extension is not supported by
                                the server.
        """

    def rcpt(self, recip: str, options: Sequence[str] = ()) -> _Reply:
        """SMTP 'rcpt' command -- indicates 1 recipient for this mail."""

    def data(self, msg: ReadableBuffer | str) -> _Reply:
        """SMTP 'DATA' command -- sends message data to server.

        Automatically quotes lines beginning with a period per rfc821.
        Raises SMTPDataError if there is an unexpected reply to the
        DATA command; the return value from this method is the final
        response code received when the all data is sent.  If msg
        is a string, lone '\\r' and '\\n' characters are converted to
        '\\r\\n' characters.  If msg is bytes, it is transmitted as is.
        """

    def verify(self, address: str) -> _Reply:
        """SMTP 'verify' command -- checks for address validity."""
    vrfy = verify
    def expn(self, address: str) -> _Reply:
        """SMTP 'expn' command -- expands a mailing list."""

    def ehlo_or_helo_if_needed(self) -> None:
        """Call self.ehlo() and/or self.helo() if needed.

        If there has been no previous EHLO or HELO command this session, this
        method tries ESMTP EHLO first.

        This method may raise the following exceptions:

         SMTPHeloError            The server didn't reply properly to
                                  the helo greeting.
        """
    user: str
    password: str
    def auth(self, mechanism: str, authobject: _AuthObject, *, initial_response_ok: bool = True) -> _Reply:
        """Authentication command - requires response processing.

        'mechanism' specifies which authentication mechanism is to
        be used - the valid values are those listed in the 'auth'
        element of 'esmtp_features'.

        'authobject' must be a callable object taking a single argument:

                data = authobject(challenge)

        It will be called to process the server's challenge response; the
        challenge argument it is passed will be a bytes.  It should return
        an ASCII string that will be base64 encoded and sent to the server.

        Keyword arguments:
            - initial_response_ok: Allow sending the RFC 4954 initial-response
              to the AUTH command, if the authentication methods supports it.
        """

    @overload
    def auth_cram_md5(self, challenge: None = None) -> None:
        """Authobject to use with CRAM-MD5 authentication. Requires self.user
        and self.password to be set.
        """

    @overload
    def auth_cram_md5(self, challenge: ReadableBuffer) -> str: ...
    def auth_plain(self, challenge: ReadableBuffer | None = None) -> str:
        """Authobject to use with PLAIN authentication. Requires self.user and
        self.password to be set.
        """

    def auth_login(self, challenge: ReadableBuffer | None = None) -> str:
        """Authobject to use with LOGIN authentication. Requires self.user and
        self.password to be set.
        """

    def login(self, user: str, password: str, *, initial_response_ok: bool = True) -> _Reply:
        """Log in on an SMTP server that requires authentication.

        The arguments are:
            - user:         The user name to authenticate with.
            - password:     The password for the authentication.

        Keyword arguments:
            - initial_response_ok: Allow sending the RFC 4954 initial-response
              to the AUTH command, if the authentication methods supports it.

        If there has been no previous EHLO or HELO command this session, this
        method tries ESMTP EHLO first.

        This method will return normally if the authentication was successful.

        This method may raise the following exceptions:

         SMTPHeloError            The server didn't reply properly to
                                  the helo greeting.
         SMTPAuthenticationError  The server didn't accept the username/
                                  password combination.
         SMTPNotSupportedError    The AUTH command is not supported by the
                                  server.
         SMTPException            No suitable authentication method was
                                  found.
        """
    if sys.version_info >= (3, 12):
        def starttls(self, *, context: SSLContext | None = None) -> _Reply:
            """Puts the connection to the SMTP server into TLS mode.

            If there has been no previous EHLO or HELO command this session, this
            method tries ESMTP EHLO first.

            If the server supports TLS, this will encrypt the rest of the SMTP
            session. If you provide the context parameter,
            the identity of the SMTP server and client can be checked. This,
            however, depends on whether the socket module really checks the
            certificates.

            This method may raise the following exceptions:

             SMTPHeloError            The server didn't reply properly to
                                      the helo greeting.
            """
    else:
        def starttls(self, keyfile: str | None = None, certfile: str | None = None, context: SSLContext | None = None) -> _Reply:
            """Puts the connection to the SMTP server into TLS mode.

            If there has been no previous EHLO or HELO command this session, this
            method tries ESMTP EHLO first.

            If the server supports TLS, this will encrypt the rest of the SMTP
            session. If you provide the keyfile and certfile parameters,
            the identity of the SMTP server and client can be checked. This,
            however, depends on whether the socket module really checks the
            certificates.

            This method may raise the following exceptions:

             SMTPHeloError            The server didn't reply properly to
                                      the helo greeting.
            """

    def sendmail(
        self,
        from_addr: str,
        to_addrs: str | Sequence[str],
        msg: SizedBuffer | str,
        mail_options: Sequence[str] = (),
        rcpt_options: Sequence[str] = (),
    ) -> _SendErrs:
        """This command performs an entire mail transaction.

The arguments are:
    - from_addr    : The address sending this mail.
    - to_addrs     : A list of addresses to send this mail to.  A bare
                     string will be treated as a list with 1 address.
    - msg          : The message to send.
    - mail_options : List of ESMTP options (such as 8bitmime) for the
                     mail command.
    - rcpt_options : List of ESMTP options (such as DSN commands) for
                     all the rcpt commands.

msg may be a string containing characters in the ASCII range, or a byte
string.  A string is encoded to bytes using the ascii codec, and lone
\\r and \\n characters are converted to \\r\\n characters.

If there has been no previous EHLO or HELO command this session, this
method tries ESMTP EHLO first.  If the server does ESMTP, message size
and each of the specified options will be passed to it.  If EHLO
fails, HELO will be tried and ESMTP options suppressed.

This method will return normally if the mail is accepted for at least
one recipient.  It returns a dictionary, with one entry for each
recipient that was refused.  Each entry contains a tuple of the SMTP
error code and the accompanying error message sent by the server.

This method may raise the following exceptions:

 SMTPHeloError          The server didn't reply properly to
                        the helo greeting.
 SMTPRecipientsRefused  The server rejected ALL recipients
                        (no mail was sent).
 SMTPSenderRefused      The server didn't accept the from_addr.
 SMTPDataError          The server replied with an unexpected
                        error code (other than a refusal of
                        a recipient).
 SMTPNotSupportedError  The mail_options parameter includes 'SMTPUTF8'
                        but the SMTPUTF8 extension is not supported by
                        the server.

Note: the connection will be open even after an exception is raised.

Example:

 >>> import smtplib
 >>> s=smtplib.SMTP("localhost")
 >>> tolist=["one@one.org","two@two.org","three@three.org","four@four.org"]
 >>> msg = '''\\
 ... From: Me@my.org
 ... Subject: testin'...
 ...
 ... This is a test '''
 >>> s.sendmail("me@my.org",tolist,msg)
 { "three@three.org" : ( 550 ,"User unknown" ) }
 >>> s.quit()

In the above example, the message was accepted for delivery to three
of the four addresses, and one was rejected, with the error code
550.  If all addresses are accepted, then the method will return an
empty dictionary.

"""

    def send_message(
        self,
        msg: _Message,
        from_addr: str | None = None,
        to_addrs: str | Sequence[str] | None = None,
        mail_options: Sequence[str] = (),
        rcpt_options: Sequence[str] = (),
    ) -> _SendErrs:
        """Converts message to a bytestring and passes it to sendmail.

        The arguments are as for sendmail, except that msg is an
        email.message.Message object.  If from_addr is None or to_addrs is
        None, these arguments are taken from the headers of the Message as
        described in RFC 5322 (a ValueError is raised if there is more than
        one set of 'Resent-' headers).  Regardless of the values of from_addr and
        to_addr, any Bcc field (or Resent-Bcc field, when the Message is a
        resent) of the Message object won't be transmitted.  The Message
        object is then serialized using email.generator.BytesGenerator and
        sendmail is called to transmit the message.  If the sender or any of
        the recipient addresses contain non-ASCII and the server advertises the
        SMTPUTF8 capability, the policy is cloned with utf8 set to True for the
        serialization, and SMTPUTF8 and BODY=8BITMIME are asserted on the send.
        If the server does not support SMTPUTF8, an SMTPNotSupported error is
        raised.  Otherwise the generator is called without modifying the
        policy.

        """

    def close(self) -> None:
        """Close the connection to the SMTP server."""

    def quit(self) -> _Reply:
        """Terminate the SMTP session."""

class SMTP_SSL(SMTP):
    """This is a subclass derived from SMTP that connects over an SSL
    encrypted socket (to use this class you need a socket module that was
    compiled with SSL support). If host is not specified, '' (the local
    host) is used. If port is omitted, the standard SMTP-over-SSL port
    (465) is used.  local_hostname and source_address have the same meaning
    as they do in the SMTP class.  context also optional, can contain a
    SSLContext.

    """

    keyfile: str | None
    certfile: str | None
    context: SSLContext
    if sys.version_info >= (3, 12):
        def __init__(
            self,
            host: str = "",
            port: int = 0,
            local_hostname: str | None = None,
            *,
            timeout: float = ...,
            source_address: _SourceAddress | None = None,
            context: SSLContext | None = None,
        ) -> None: ...
    else:
        def __init__(
            self,
            host: str = "",
            port: int = 0,
            local_hostname: str | None = None,
            keyfile: str | None = None,
            certfile: str | None = None,
            timeout: float = ...,
            source_address: _SourceAddress | None = None,
            context: SSLContext | None = None,
        ) -> None: ...

LMTP_PORT: Final = 2003

class LMTP(SMTP):
    """LMTP - Local Mail Transfer Protocol

    The LMTP protocol, which is very similar to ESMTP, is heavily based
    on the standard SMTP client. It's common to use Unix sockets for
    LMTP, so our connect() method must support that as well as a regular
    host:port server.  local_hostname and source_address have the same
    meaning as they do in the SMTP class.  To specify a Unix socket,
    you must use an absolute path as the host, starting with a '/'.

    Authentication is supported, using the regular SMTP mechanism. When
    using a Unix socket, LMTP generally don't support or require any
    authentication, but your mileage might vary.
    """

    def __init__(
        self,
        host: str = "",
        port: int = 2003,
        local_hostname: str | None = None,
        source_address: _SourceAddress | None = None,
        timeout: float = ...,
    ) -> None:
        """Initialize a new instance."""
