"""
Generic framework path manipulation
"""

from typing import TypedDict, type_check_only

__all__ = ["framework_info"]

# Actual result is produced by re.match.groupdict()
@type_check_only
class _FrameworkInfo(TypedDict):
    location: str
    name: str
    shortname: str
    version: str | None
    suffix: str | None

def framework_info(filename: str) -> _FrameworkInfo | None:
    """
    A framework name can take one of the following four forms:
        Location/Name.framework/Versions/SomeVersion/Name_Suffix
        Location/Name.framework/Versions/SomeVersion/Name
        Location/Name.framework/Name_Suffix
        Location/Name.framework/Name

    returns None if not found, or a mapping equivalent to:
        dict(
            location='Location',
            name='Name.framework/Versions/SomeVersion/Name_Suffix',
            shortname='Name',
            version='SomeVersion',
            suffix='Suffix',
        )

    Note that SomeVersion and Suffix are optional and may be None
    if not present
    """
