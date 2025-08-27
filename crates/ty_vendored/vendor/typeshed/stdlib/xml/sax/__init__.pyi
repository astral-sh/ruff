"""Simple API for XML (SAX) implementation for Python.

This module provides an implementation of the SAX 2 interface;
information about the Java version of the interface can be found at
http://www.megginson.com/SAX/.  The Python version of the interface is
documented at <...>.

This package contains the following modules:

handler -- Base classes and constants which define the SAX 2 API for
           the 'client-side' of SAX for Python.

saxutils -- Implementation of the convenience classes commonly used to
            work with SAX.

xmlreader -- Base classes and constants which define the SAX 2 API for
             the parsers used with SAX for Python.

expatreader -- Driver that allows use of the Expat parser with SAX.
"""

import sys
from _typeshed import ReadableBuffer, StrPath, SupportsRead, _T_co
from collections.abc import Iterable
from typing import Final, Protocol, type_check_only
from typing_extensions import TypeAlias
from xml.sax._exceptions import (
    SAXException as SAXException,
    SAXNotRecognizedException as SAXNotRecognizedException,
    SAXNotSupportedException as SAXNotSupportedException,
    SAXParseException as SAXParseException,
    SAXReaderNotAvailable as SAXReaderNotAvailable,
)
from xml.sax.handler import ContentHandler as ContentHandler, ErrorHandler as ErrorHandler
from xml.sax.xmlreader import InputSource as InputSource, XMLReader

@type_check_only
class _SupportsReadClose(SupportsRead[_T_co], Protocol[_T_co]):
    def close(self) -> None: ...

_Source: TypeAlias = StrPath | _SupportsReadClose[bytes] | _SupportsReadClose[str]

default_parser_list: Final[list[str]]

def make_parser(parser_list: Iterable[str] = ()) -> XMLReader:
    """Creates and returns a SAX parser.

    Creates the first parser it is able to instantiate of the ones
    given in the iterable created by chaining parser_list and
    default_parser_list.  The iterables must contain the names of Python
    modules containing both a SAX parser and a create_parser function.
    """

def parse(source: _Source, handler: ContentHandler, errorHandler: ErrorHandler = ...) -> None: ...
def parseString(string: ReadableBuffer | str, handler: ContentHandler, errorHandler: ErrorHandler | None = ...) -> None: ...
def _create_parser(parser_name: str) -> XMLReader: ...

if sys.version_info >= (3, 14):
    __all__ = [
        "ContentHandler",
        "ErrorHandler",
        "InputSource",
        "SAXException",
        "SAXNotRecognizedException",
        "SAXNotSupportedException",
        "SAXParseException",
        "SAXReaderNotAvailable",
        "default_parser_list",
        "make_parser",
        "parse",
        "parseString",
    ]
