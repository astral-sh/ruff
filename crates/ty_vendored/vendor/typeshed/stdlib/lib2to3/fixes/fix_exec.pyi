"""Fixer for exec.

This converts usages of the exec statement into calls to a built-in
exec() function.

exec code in ns1, ns2 -> exec(code, ns1, ns2)
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixExec(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
