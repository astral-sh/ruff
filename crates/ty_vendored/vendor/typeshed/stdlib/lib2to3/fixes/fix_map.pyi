"""Fixer that changes map(F, ...) into list(map(F, ...)) unless there
exists a 'from future_builtins import map' statement in the top-level
namespace.

As a special case, map(None, X) is changed into list(X).  (This is
necessary because the semantics are changed in this case -- the new
map(None, X) is equivalent to [(x,) for x in X].)

We avoid the transformation (except for the special case mentioned
above) if the map() call is directly contained in iter(<>), list(<>),
tuple(<>), sorted(<>), ...join(<>), or for V in <>:.

NOTE: This is still not correct if the original code was depending on
map(F, X, Y, ...) to go on until the longest argument is exhausted,
substituting None for missing values -- like zip(), it now stops as
soon as the shortest argument is exhausted.
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixMap(fixer_base.ConditionalFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    skip_on: ClassVar[Literal["future_builtins.map"]]
    def transform(self, node, results): ...
