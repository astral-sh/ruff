"""Different kinds of SAX Exceptions"""

from typing import NoReturn
from xml.sax.xmlreader import Locator

class SAXException(Exception):
    """Encapsulate an XML error or warning. This class can contain
    basic error or warning information from either the XML parser or
    the application: you can subclass it to provide additional
    functionality, or to add localization. Note that although you will
    receive a SAXException as the argument to the handlers in the
    ErrorHandler interface, you are not actually required to raise
    the exception; instead, you can simply read the information in
    it.
    """

    def __init__(self, msg: str, exception: Exception | None = None) -> None:
        """Creates an exception. The message is required, but the exception
        is optional.
        """

    def getMessage(self) -> str:
        """Return a message for this exception."""

    def getException(self) -> Exception | None:
        """Return the embedded exception, or None if there was none."""

    def __getitem__(self, ix: object) -> NoReturn:
        """Avoids weird error messages if someone does exception[ix] by
        mistake, since Exception has __getitem__ defined.
        """

class SAXParseException(SAXException):
    """Encapsulate an XML parse error or warning.

    This exception will include information for locating the error in
    the original XML document. Note that although the application will
    receive a SAXParseException as the argument to the handlers in the
    ErrorHandler interface, the application is not actually required
    to raise the exception; instead, it can simply read the
    information in it and take a different action.

    Since this exception is a subclass of SAXException, it inherits
    the ability to wrap another exception.
    """

    def __init__(self, msg: str, exception: Exception | None, locator: Locator) -> None:
        """Creates the exception. The exception parameter is allowed to be None."""

    def getColumnNumber(self) -> int | None:
        """The column number of the end of the text where the exception
        occurred.
        """

    def getLineNumber(self) -> int | None:
        """The line number of the end of the text where the exception occurred."""

    def getPublicId(self) -> str | None:
        """Get the public identifier of the entity where the exception occurred."""

    def getSystemId(self) -> str | None:
        """Get the system identifier of the entity where the exception occurred."""

class SAXNotRecognizedException(SAXException):
    """Exception class for an unrecognized identifier.

    An XMLReader will raise this exception when it is confronted with an
    unrecognized feature or property. SAX applications and extensions may
    use this class for similar purposes.
    """

class SAXNotSupportedException(SAXException):
    """Exception class for an unsupported operation.

    An XMLReader will raise this exception when a service it cannot
    perform is requested (specifically setting a state or value). SAX
    applications and extensions may use this class for similar
    purposes.
    """

class SAXReaderNotAvailable(SAXNotSupportedException):
    """Exception class for a missing driver.

    An XMLReader module (driver) should raise this exception when it
    is first imported, e.g. when a support module cannot be imported.
    It also may be raised during parsing, e.g. if executing an external
    program is not permitted.
    """
