from typing import Generic, TypeVar

# this case should not be affected by UP051 alone, but with UP046 and PYI018,
# the old-style generics should be replaced with type parameters, the private
# type parameters renamed without underscores, and then the standalone _T
# removed
_T = TypeVar("_T")


class OldStyle(Generic[_T]):
    var: _T
