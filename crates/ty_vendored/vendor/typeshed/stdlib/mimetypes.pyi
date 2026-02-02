"""Guess the MIME type of a file.

This module defines two useful functions:

guess_type(url, strict=True) -- guess the MIME type and encoding of a URL.

guess_extension(type, strict=True) -- guess the extension for a given MIME type.

It also contains the following, for tuning the behavior:

Data:

knownfiles -- list of files to parse
inited -- flag set when init() has been called
suffix_map -- dictionary mapping suffixes to suffixes
encodings_map -- dictionary mapping suffixes to encodings
types_map -- dictionary mapping suffixes to types

Functions:

init([files]) -- parse a list of files, default knownfiles (on Windows, the
  default values are taken from the registry)
read_mime_types(file) -- parse one file, return a dictionary or None
"""

import sys
from _typeshed import StrPath
from collections.abc import Iterable
from typing import IO

__all__ = [
    "knownfiles",
    "inited",
    "MimeTypes",
    "guess_type",
    "guess_all_extensions",
    "guess_extension",
    "add_type",
    "init",
    "read_mime_types",
    "suffix_map",
    "encodings_map",
    "types_map",
    "common_types",
]

if sys.version_info >= (3, 13):
    __all__ += ["guess_file_type"]

def guess_type(url: StrPath, strict: bool = True) -> tuple[str | None, str | None]:
    """Guess the type of a file based on its URL.

    Return value is a tuple (type, encoding) where type is None if the
    type can't be guessed (no or unknown suffix) or a string of the
    form type/subtype, usable for a MIME Content-type header; and
    encoding is None for no encoding or the name of the program used
    to encode (e.g. compress or gzip).  The mappings are table
    driven.  Encoding suffixes are case sensitive; type suffixes are
    first tried case sensitive, then case insensitive.

    The suffixes .tgz, .taz and .tz (case sensitive!) are all mapped
    to ".tar.gz".  (This is table-driven too, using the dictionary
    suffix_map).

    Optional 'strict' argument when false adds a bunch of commonly found, but
    non-standard types.
    """

def guess_all_extensions(type: str, strict: bool = True) -> list[str]:
    """Guess the extensions for a file based on its MIME type.

    Return value is a list of strings giving the possible filename
    extensions, including the leading dot ('.').  The extension is not
    guaranteed to have been associated with any particular data
    stream, but would be mapped to the MIME type 'type' by
    guess_type().  If no extension can be guessed for 'type', None
    is returned.

    Optional 'strict' argument when false adds a bunch of commonly found,
    but non-standard types.
    """

def guess_extension(type: str, strict: bool = True) -> str | None:
    """Guess the extension for a file based on its MIME type.

    Return value is a string giving a filename extension, including the
    leading dot ('.').  The extension is not guaranteed to have been
    associated with any particular data stream, but would be mapped to the
    MIME type 'type' by guess_type().  If no extension can be guessed for
    'type', None is returned.

    Optional 'strict' argument when false adds a bunch of commonly found,
    but non-standard types.
    """

def init(files: Iterable[StrPath] | None = None) -> None: ...
def read_mime_types(file: StrPath) -> dict[str, str] | None: ...
def add_type(type: str, ext: str, strict: bool = True) -> None:
    """Add a mapping between a type and an extension.

    When the extension is already known, the new
    type will replace the old one. When the type
    is already known the extension will be added
    to the list of known extensions.

    If strict is true, information will be added to
    list of standard types, else to the list of non-standard
    types.
    """

if sys.version_info >= (3, 13):
    def guess_file_type(path: StrPath, *, strict: bool = True) -> tuple[str | None, str | None]:
        """Guess the type of a file based on its path.

        Similar to guess_type(), but takes file path instead of URL.
        """

inited: bool
knownfiles: list[StrPath]
suffix_map: dict[str, str]
encodings_map: dict[str, str]
types_map: dict[str, str]
common_types: dict[str, str]

class MimeTypes:
    """MIME-types datastore.

    This datastore can handle information from mime.types-style files
    and supports basic determination of MIME type from a filename or
    URL, and can guess a reasonable extension given a MIME type.
    """

    suffix_map: dict[str, str]
    encodings_map: dict[str, str]
    types_map: tuple[dict[str, str], dict[str, str]]
    types_map_inv: tuple[dict[str, str], dict[str, str]]
    def __init__(self, filenames: Iterable[StrPath] = (), strict: bool = True) -> None: ...
    def add_type(self, type: str, ext: str, strict: bool = True) -> None:
        """Add a mapping between a type and an extension.

        When the extension is already known, the new
        type will replace the old one. When the type
        is already known the extension will be added
        to the list of known extensions.

        If strict is true, information will be added to
        list of standard types, else to the list of non-standard
        types.

        Valid extensions are empty or start with a '.'.
        """

    def guess_extension(self, type: str, strict: bool = True) -> str | None:
        """Guess the extension for a file based on its MIME type.

        Return value is a string giving a filename extension,
        including the leading dot ('.').  The extension is not
        guaranteed to have been associated with any particular data
        stream, but would be mapped to the MIME type 'type' by
        guess_type().  If no extension can be guessed for 'type', None
        is returned.

        Optional 'strict' argument when false adds a bunch of commonly found,
        but non-standard types.
        """

    def guess_type(self, url: StrPath, strict: bool = True) -> tuple[str | None, str | None]:
        """Guess the type of a file which is either a URL or a path-like object.

        Return value is a tuple (type, encoding) where type is None if
        the type can't be guessed (no or unknown suffix) or a string
        of the form type/subtype, usable for a MIME Content-type
        header; and encoding is None for no encoding or the name of
        the program used to encode (e.g. compress or gzip).  The
        mappings are table driven.  Encoding suffixes are case
        sensitive; type suffixes are first tried case sensitive, then
        case insensitive.

        The suffixes .tgz, .taz and .tz (case sensitive!) are all
        mapped to '.tar.gz'.  (This is table-driven too, using the
        dictionary suffix_map.)

        Optional 'strict' argument when False adds a bunch of commonly found,
        but non-standard types.
        """

    def guess_all_extensions(self, type: str, strict: bool = True) -> list[str]:
        """Guess the extensions for a file based on its MIME type.

        Return value is a list of strings giving the possible filename
        extensions, including the leading dot ('.').  The extension is not
        guaranteed to have been associated with any particular data stream,
        but would be mapped to the MIME type 'type' by guess_type().

        Optional 'strict' argument when false adds a bunch of commonly found,
        but non-standard types.
        """

    def read(self, filename: StrPath, strict: bool = True) -> None:
        """
        Read a single mime.types-format file, specified by pathname.

        If strict is true, information will be added to
        list of standard types, else to the list of non-standard
        types.
        """

    def readfp(self, fp: IO[str], strict: bool = True) -> None:
        """
        Read a single mime.types-format file.

        If strict is true, information will be added to
        list of standard types, else to the list of non-standard
        types.
        """

    def read_windows_registry(self, strict: bool = True) -> None:
        """
        Load the MIME types database from Windows registry.

        If strict is true, information will be added to
        list of standard types, else to the list of non-standard
        types.
        """
    if sys.version_info >= (3, 13):
        def guess_file_type(self, path: StrPath, *, strict: bool = True) -> tuple[str | None, str | None]:
            """Guess the type of a file based on its path.

            Similar to guess_type(), but takes file path instead of URL.
            """
