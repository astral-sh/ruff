"""Replacing AnyStr requires specifying the constraints `bytes` and `str`, so
it can't be replaced if these have been shadowed. This test is in a separate
fixture because it doesn't seem possible to restore `str` to its builtin state
"""

from typing import AnyStr, Generic

str = "string"


class BadStr(Generic[AnyStr]):
    var: AnyStr
