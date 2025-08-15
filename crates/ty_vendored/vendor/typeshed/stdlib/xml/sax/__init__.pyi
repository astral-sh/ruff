import sys
from _typeshed import ReadableBuffer, StrPath, SupportsRead, _T_co
from collections.abc import Iterable
from typing import Protocol, type_check_only
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

default_parser_list: list[str]

def make_parser(parser_list: Iterable[str] = ()) -> XMLReader: ...
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
