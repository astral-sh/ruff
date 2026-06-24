"""
Generic dylib path manipulation
"""

from typing import TypedDict, type_check_only

__all__ = ["dylib_info"]

# Actual result is produced by re.match.groupdict()
@type_check_only
class _DylibInfo(TypedDict):
    location: str
    name: str
    shortname: str
    version: str | None
    suffix: str | None

def dylib_info(filename: str) -> _DylibInfo | None:
    """
    A dylib name can take one of the following four forms:
        Location/Name.SomeVersion_Suffix.dylib
        Location/Name.SomeVersion.dylib
        Location/Name_Suffix.dylib
        Location/Name.dylib

    returns None if not found or a mapping equivalent to:
        dict(
            location='Location',
            name='Name.SomeVersion_Suffix.dylib',
            shortname='Name',
            version='SomeVersion',
            suffix='Suffix',
        )

    Note that SomeVersion and Suffix are optional and may be None
    if not present.
    """
