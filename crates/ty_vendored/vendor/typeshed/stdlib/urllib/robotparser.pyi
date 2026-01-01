"""robotparser.py

Copyright (C) 2000  Bastian Kleineidam

You can choose between two licenses when using this package:
1) GNU GPLv2
2) PSF license for Python 2.2

The robots.txt Exclusion Protocol is implemented as specified in
http://www.robotstxt.org/norobots-rfc.txt
"""

from collections.abc import Iterable
from typing import NamedTuple

__all__ = ["RobotFileParser"]

class RequestRate(NamedTuple):
    """RequestRate(requests, seconds)"""

    requests: int
    seconds: int

class RobotFileParser:
    """This class provides a set of methods to read, parse and answer
    questions about a single robots.txt file.

    """

    def __init__(self, url: str = "") -> None: ...
    def set_url(self, url: str) -> None:
        """Sets the URL referring to a robots.txt file."""

    def read(self) -> None:
        """Reads the robots.txt URL and feeds it to the parser."""

    def parse(self, lines: Iterable[str]) -> None:
        """Parse the input lines from a robots.txt file.

        We allow that a user-agent: line is not preceded by
        one or more blank lines.
        """

    def can_fetch(self, useragent: str, url: str) -> bool:
        """using the parsed robots.txt decide if useragent can fetch url"""

    def mtime(self) -> int:
        """Returns the time the robots.txt file was last fetched.

        This is useful for long-running web spiders that need to
        check for new robots.txt files periodically.

        """

    def modified(self) -> None:
        """Sets the time the robots.txt file was last fetched to the
        current time.

        """

    def crawl_delay(self, useragent: str) -> str | None: ...
    def request_rate(self, useragent: str) -> RequestRate | None: ...
    def site_maps(self) -> list[str] | None: ...
