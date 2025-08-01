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

def dylib_info(filename: str) -> _DylibInfo | None: ...
