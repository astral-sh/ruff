"""A parser for HTML and XHTML."""

from _markupbase import ParserBase
from re import Pattern
from typing import Final

__all__ = ["HTMLParser"]

class HTMLParser(ParserBase):
    """Find tags and other markup and call handler functions.

    Usage:
        p = HTMLParser()
        p.feed(data)
        ...
        p.close()

    Start tags are handled by calling self.handle_starttag() or
    self.handle_startendtag(); end tags by self.handle_endtag().  The
    data between tags is passed from the parser to the derived class
    by calling self.handle_data() with the data as argument (the data
    may be split up in arbitrary chunks).  If convert_charrefs is
    True the character references are converted automatically to the
    corresponding Unicode character (and self.handle_data() is no
    longer split in chunks), otherwise they are passed by calling
    self.handle_entityref() or self.handle_charref() with the string
    containing respectively the named or numeric reference as the
    argument.
    """

    CDATA_CONTENT_ELEMENTS: Final[tuple[str, ...]]
    # Added in Python 3.9.23, 3.10.18, 3.11.13, 3.12.11, 3.13.6
    RCDATA_CONTENT_ELEMENTS: Final[tuple[str, ...]]

    # `scripting` parameter added in Python 3.9.25, 3.10.20, 3.11.15, 3.12.13, 3.13.10, 3.14.1
    def __init__(self, *, convert_charrefs: bool = True, scripting: bool = False) -> None:
        """Initialize and reset this instance.

        If convert_charrefs is true (the default), all character references
        are automatically converted to the corresponding Unicode characters.

        If *scripting* is false (the default), the content of the
        ``noscript`` element is parsed normally; if it's true,
        it's returned as is without being parsed.
        """

    def feed(self, data: str) -> None:
        """Feed data to the parser.

        Call this as often as you want, with as little or as much text
        as you want (may include '\\n').
        """

    def close(self) -> None:
        """Handle any buffered data."""

    def get_starttag_text(self) -> str | None:
        """Return full source of start tag: '<...>'."""

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None: ...
    def handle_endtag(self, tag: str) -> None: ...
    def handle_startendtag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None: ...
    def handle_data(self, data: str) -> None: ...
    def handle_entityref(self, name: str) -> None: ...
    def handle_charref(self, name: str) -> None: ...
    def handle_comment(self, data: str) -> None: ...
    def handle_decl(self, decl: str) -> None: ...
    def handle_pi(self, data: str) -> None: ...
    def check_for_whole_start_tag(self, i: int) -> int: ...  # undocumented
    def clear_cdata_mode(self) -> None: ...  # undocumented
    def goahead(self, end: bool) -> None: ...  # undocumented
    def parse_bogus_comment(self, i: int, report: bool = True) -> int: ...  # undocumented
    def parse_endtag(self, i: int) -> int: ...  # undocumented
    def parse_html_declaration(self, i: int) -> int: ...  # undocumented
    def parse_pi(self, i: int) -> int: ...  # undocumented
    def parse_starttag(self, i: int) -> int: ...  # undocumented
    # `escapable` parameter added in Python 3.9.23, 3.10.18, 3.11.13, 3.12.11, 3.13.6
    def set_cdata_mode(self, elem: str, *, escapable: bool = False) -> None: ...  # undocumented
    rawdata: str  # undocumented
    cdata_elem: str | None  # undocumented
    convert_charrefs: bool  # undocumented
    interesting: Pattern[str]  # undocumented
    lasttag: str  # undocumented
