"""Fixer for sys.exc_{type, value, traceback}

sys.exc_type -> sys.exc_info()[0]
sys.exc_value -> sys.exc_info()[1]
sys.exc_traceback -> sys.exc_info()[2]
"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixSysExc(fixer_base.BaseFix):
    exc_info: ClassVar[list[str]]
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
