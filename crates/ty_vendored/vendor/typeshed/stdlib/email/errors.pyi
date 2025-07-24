"""email package exception classes."""

import sys

class MessageError(Exception):
    """Base class for errors in the email package."""

class MessageParseError(MessageError):
    """Base class for message parsing errors."""

class HeaderParseError(MessageParseError):
    """Error while parsing headers."""

class BoundaryError(MessageParseError):
    """Couldn't find terminating boundary."""

class MultipartConversionError(MessageError, TypeError):
    """Conversion to a multipart is prohibited."""

class CharsetError(MessageError):
    """An illegal charset was given."""

# Added in Python 3.9.20, 3.10.15, 3.11.10, 3.12.5
class HeaderWriteError(MessageError):
    """Error while writing headers."""

class MessageDefect(ValueError):
    """Base class for a message defect."""

    def __init__(self, line: str | None = None) -> None: ...

class NoBoundaryInMultipartDefect(MessageDefect):
    """A message claimed to be a multipart but had no boundary parameter."""

class StartBoundaryNotFoundDefect(MessageDefect):
    """The claimed start boundary was never found."""

class FirstHeaderLineIsContinuationDefect(MessageDefect):
    """A message had a continuation line as its first header line."""

class MisplacedEnvelopeHeaderDefect(MessageDefect):
    """A 'Unix-from' header was found in the middle of a header block."""

class MultipartInvariantViolationDefect(MessageDefect):
    """A message claimed to be a multipart but no subparts were found."""

class InvalidMultipartContentTransferEncodingDefect(MessageDefect):
    """An invalid content transfer encoding was set on the multipart itself."""

class UndecodableBytesDefect(MessageDefect):
    """Header contained bytes that could not be decoded"""

class InvalidBase64PaddingDefect(MessageDefect):
    """base64 encoded sequence had an incorrect length"""

class InvalidBase64CharactersDefect(MessageDefect):
    """base64 encoded sequence had characters not in base64 alphabet"""

class InvalidBase64LengthDefect(MessageDefect):
    """base64 encoded sequence had invalid length (1 mod 4)"""

class CloseBoundaryNotFoundDefect(MessageDefect):
    """A start boundary was found, but not the corresponding close boundary."""

class MissingHeaderBodySeparatorDefect(MessageDefect):
    """Found line with no leading whitespace and no colon before blank line."""

MalformedHeaderDefect = MissingHeaderBodySeparatorDefect

class HeaderDefect(MessageDefect):
    """Base class for a header defect."""

class InvalidHeaderDefect(HeaderDefect):
    """Header is not valid, message gives details."""

class HeaderMissingRequiredValue(HeaderDefect):
    """A header that must have a value had none"""

class NonPrintableDefect(HeaderDefect):
    """ASCII characters outside the ascii-printable range found"""

    def __init__(self, non_printables: str | None) -> None: ...

class ObsoleteHeaderDefect(HeaderDefect):
    """Header uses syntax declared obsolete by RFC 5322"""

class NonASCIILocalPartDefect(HeaderDefect):
    """local_part contains non-ASCII characters"""

if sys.version_info >= (3, 10):
    class InvalidDateDefect(HeaderDefect):
        """Header has unparsable or invalid date"""
