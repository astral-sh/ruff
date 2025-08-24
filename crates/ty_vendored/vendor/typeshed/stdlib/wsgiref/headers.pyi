"""Manage HTTP Response Headers

Much of this module is red-handedly pilfered from email.message in the stdlib,
so portions are Copyright (C) 2001 Python Software Foundation, and were
written by Barry Warsaw.
"""

from re import Pattern
from typing import Final, overload
from typing_extensions import TypeAlias

_HeaderList: TypeAlias = list[tuple[str, str]]

tspecials: Final[Pattern[str]]  # undocumented

class Headers:
    """Manage a collection of HTTP response headers"""

    def __init__(self, headers: _HeaderList | None = None) -> None: ...
    def __len__(self) -> int:
        """Return the total number of headers, including duplicates."""

    def __setitem__(self, name: str, val: str) -> None:
        """Set the value of a header."""

    def __delitem__(self, name: str) -> None:
        """Delete all occurrences of a header, if present.

        Does *not* raise an exception if the header is missing.
        """

    def __getitem__(self, name: str) -> str | None:
        """Get the first header value for 'name'

        Return None if the header is missing instead of raising an exception.

        Note that if the header appeared multiple times, the first exactly which
        occurrence gets returned is undefined.  Use getall() to get all
        the values matching a header field name.
        """

    def __contains__(self, name: str) -> bool:
        """Return true if the message contains the header."""

    def get_all(self, name: str) -> list[str]:
        """Return a list of all the values for the named field.

        These will be sorted in the order they appeared in the original header
        list or were added to this instance, and may contain duplicates.  Any
        fields deleted and re-inserted are always appended to the header list.
        If no fields exist with the given name, returns an empty list.
        """

    @overload
    def get(self, name: str, default: str) -> str:
        """Get the first header value for 'name', or return 'default'"""

    @overload
    def get(self, name: str, default: str | None = None) -> str | None: ...
    def keys(self) -> list[str]:
        """Return a list of all the header field names.

        These will be sorted in the order they appeared in the original header
        list, or were added to this instance, and may contain duplicates.
        Any fields deleted and re-inserted are always appended to the header
        list.
        """

    def values(self) -> list[str]:
        """Return a list of all header values.

        These will be sorted in the order they appeared in the original header
        list, or were added to this instance, and may contain duplicates.
        Any fields deleted and re-inserted are always appended to the header
        list.
        """

    def items(self) -> _HeaderList:
        """Get all the header fields and values.

        These will be sorted in the order they were in the original header
        list, or were added to this instance, and may contain duplicates.
        Any fields deleted and re-inserted are always appended to the header
        list.
        """

    def __bytes__(self) -> bytes: ...
    def setdefault(self, name: str, value: str) -> str:
        """Return first matching header value for 'name', or 'value'

        If there is no header named 'name', add a new header with name 'name'
        and value 'value'.
        """

    def add_header(self, _name: str, _value: str | None, **_params: str | None) -> None:
        """Extended header setting.

        _name is the header field to add.  keyword arguments can be used to set
        additional parameters for the header field, with underscores converted
        to dashes.  Normally the parameter will be added as key="value" unless
        value is None, in which case only the key will be added.

        Example:

        h.add_header('content-disposition', 'attachment', filename='bud.gif')

        Note that unlike the corresponding 'email.message' method, this does
        *not* handle '(charset, language, value)' tuples: all values must be
        strings or None.
        """
