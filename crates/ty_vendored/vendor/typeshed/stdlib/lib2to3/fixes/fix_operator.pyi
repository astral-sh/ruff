"""Fixer for operator functions.

operator.isCallable(obj)       -> callable(obj)
operator.sequenceIncludes(obj) -> operator.contains(obj)
operator.isSequenceType(obj)   -> isinstance(obj, collections.abc.Sequence)
operator.isMappingType(obj)    -> isinstance(obj, collections.abc.Mapping)
operator.isNumberType(obj)     -> isinstance(obj, numbers.Number)
operator.repeat(obj, n)        -> operator.mul(obj, n)
operator.irepeat(obj, n)       -> operator.imul(obj, n)
"""

from lib2to3 import fixer_base
from typing import ClassVar, Literal

def invocation(s): ...

class FixOperator(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    order: ClassVar[Literal["pre"]]
    methods: str
    obj: str
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
