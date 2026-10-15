"""This module provides access to the Unicode Character Database which
defines character properties for all Unicode characters. The data in
this database is based on the UnicodeData.txt file version
17.0.0 which is publicly available from ftp://ftp.unicode.org/.

The module uses the same names and symbols as defined by the
UnicodeData File Format 17.0.0.
"""

import sys
from _typeshed import ReadOnlyBuffer
from collections.abc import Iterator
from typing import Final, Literal, TypeAlias, TypeVar, final, overload

ucd_3_2_0: UCD
unidata_version: Final[str]

_T = TypeVar("_T")

_NormalizationForm: TypeAlias = Literal["NFC", "NFD", "NFKC", "NFKD"]

def bidirectional(chr: str, /) -> str:
    """Returns the bidirectional class assigned to the character chr as string.

    If no such value is defined, an empty string is returned.
    """

def category(chr: str, /) -> str:
    """Returns the general category assigned to the character chr as string."""

def combining(chr: str, /) -> int:
    """Returns the canonical combining class assigned to the character chr as integer.

    Returns 0 if no combining class is defined.
    """

@overload
def decimal(chr: str, /) -> int:
    """Converts a Unicode character into its equivalent decimal value.

    Returns the decimal value assigned to the character chr as integer.
    If no such value is defined, default is returned, or, if not given,
    ValueError is raised.
    """
@overload
def decimal(chr: str, default: _T, /) -> int | _T: ...

def decomposition(chr: str, /) -> str:
    """Returns the character decomposition mapping assigned to the character chr as string.

    An empty string is returned in case no such mapping is defined.
    """

@overload
def digit(chr: str, /) -> int:
    """Converts a Unicode character into its equivalent digit value.

    Returns the digit value assigned to the character chr as integer.
    If no such value is defined, default is returned, or, if not given,
    ValueError is raised.
    """
@overload
def digit(chr: str, default: _T, /) -> int | _T: ...

_EastAsianWidth: TypeAlias = Literal["F", "H", "W", "Na", "A", "N"]

def east_asian_width(chr: str, /) -> _EastAsianWidth:
    """Returns the east asian width assigned to the character chr as string."""

def is_normalized(form: _NormalizationForm, unistr: str, /) -> bool:
    """Return whether the Unicode string unistr is in the normal form 'form'.

    Valid values for form are 'NFC', 'NFKC', 'NFD', and 'NFKD'.
    """

if sys.version_info >= (3, 15):
    def block(chr: str, /) -> str:
        """Return block assigned to the character chr."""

    def extended_pictographic(chr: str, /) -> bool:
        """Returns the Extended_Pictographic property assigned to the character, as boolean."""

    def grapheme_cluster_break(chr: str, /) -> str:
        """Returns the Grapheme_Cluster_Break property assigned to the character."""

    def indic_conjunct_break(chr: str, /) -> str:
        """Returns the Indic_Conjunct_Break property assigned to the character."""

    def isxidstart(chr: str, /) -> bool:
        """Return True if the character has the XID_Start property, else False."""

    def isxidcontinue(chr: str, /) -> bool:
        """Return True if the character has the XID_Continue property, else False."""

    def iter_graphemes(unistr: str, start: int = 0, end: int = sys.maxsize, /) -> Iterator[str]:
        """Returns an iterator to iterate over grapheme clusters.

        It uses extended grapheme cluster rules from TR29.
        """

def lookup(name: str | ReadOnlyBuffer, /) -> str:
    """Look up character by name.

    If a character with the given name is found, return the
    corresponding character.  If not found, KeyError is raised.
    """

def mirrored(chr: str, /) -> int:
    """Returns the mirrored property assigned to the character chr as integer.

    Returns 1 if the character has been identified as a "mirrored"
    character in bidirectional text, 0 otherwise.
    """

@overload
def name(chr: str, /) -> str:
    """Returns the name assigned to the character chr as a string.

    If no name is defined, default is returned, or, if not given,
    ValueError is raised.
    """
@overload
def name(chr: str, default: _T, /) -> str | _T: ...

def normalize(form: _NormalizationForm, unistr: str, /) -> str:
    """Return the normal form 'form' for the Unicode string unistr.

    Valid values for form are 'NFC', 'NFKC', 'NFD', and 'NFKD'.
    """

@overload
def numeric(chr: str, /) -> float:
    """Converts a Unicode character into its equivalent numeric value.

    Returns the numeric value assigned to the character chr as float.
    If no such value is defined, default is returned, or, if not given,
    ValueError is raised.
    """
@overload
def numeric(chr: str, default: _T, /) -> float | _T: ...

@final
class UCD:
    # The methods below are constructed from the same array in C
    # (unicodedata_functions) and hence identical to the functions above.
    unidata_version: str
    def bidirectional(self, chr: str, /) -> str:
        """Returns the bidirectional class assigned to the character chr as string.

        If no such value is defined, an empty string is returned.
        """

    def category(self, chr: str, /) -> str:
        """Returns the general category assigned to the character chr as string."""

    def combining(self, chr: str, /) -> int:
        """Returns the canonical combining class assigned to the character chr as integer.

        Returns 0 if no combining class is defined.
        """

    @overload
    def decimal(self, chr: str, /) -> int:
        """Converts a Unicode character into its equivalent decimal value.

        Returns the decimal value assigned to the character chr as integer.
        If no such value is defined, default is returned, or, if not given,
        ValueError is raised.
        """
    @overload
    def decimal(self, chr: str, default: _T, /) -> int | _T: ...

    def decomposition(self, chr: str, /) -> str:
        """Returns the character decomposition mapping assigned to the character chr as string.

        An empty string is returned in case no such mapping is defined.
        """

    @overload
    def digit(self, chr: str, /) -> int:
        """Converts a Unicode character into its equivalent digit value.

        Returns the digit value assigned to the character chr as integer.
        If no such value is defined, default is returned, or, if not given,
        ValueError is raised.
        """
    @overload
    def digit(self, chr: str, default: _T, /) -> int | _T: ...

    def east_asian_width(self, chr: str, /) -> _EastAsianWidth:
        """Returns the east asian width assigned to the character chr as string."""

    def is_normalized(self, form: _NormalizationForm, unistr: str, /) -> bool:
        """Return whether the Unicode string unistr is in the normal form 'form'.

        Valid values for form are 'NFC', 'NFKC', 'NFD', and 'NFKD'.
        """

    def lookup(self, name: str | ReadOnlyBuffer, /) -> str:
        """Look up character by name.

        If a character with the given name is found, return the
        corresponding character.  If not found, KeyError is raised.
        """

    def mirrored(self, chr: str, /) -> int:
        """Returns the mirrored property assigned to the character chr as integer.

        Returns 1 if the character has been identified as a "mirrored"
        character in bidirectional text, 0 otherwise.
        """

    @overload
    def name(self, chr: str, /) -> str:
        """Returns the name assigned to the character chr as a string.

        If no name is defined, default is returned, or, if not given,
        ValueError is raised.
        """
    @overload
    def name(self, chr: str, default: _T, /) -> str | _T: ...

    def normalize(self, form: _NormalizationForm, unistr: str, /) -> str:
        """Return the normal form 'form' for the Unicode string unistr.

        Valid values for form are 'NFC', 'NFKC', 'NFD', and 'NFKD'.
        """

    @overload
    def numeric(self, chr: str, /) -> float:
        """Converts a Unicode character into its equivalent numeric value.

        Returns the numeric value assigned to the character chr as float.
        If no such value is defined, default is returned, or, if not given,
        ValueError is raised.
        """
    @overload
    def numeric(self, chr: str, default: _T, /) -> float | _T: ...
