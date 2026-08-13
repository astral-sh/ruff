"""Fixer for removing uses of the types module.

These work for only the known names in the types module.  The forms above
can include types. or not.  ie, It is assumed the module is imported either as:

    import types
    from types import ... # either * or specific types

The import statements are not modified.

There should be another fixer that handles at least the following constants:

   type([]) -> list
   type(()) -> tuple
   type('') -> str

"""

from typing import ClassVar, Literal

from .. import fixer_base

class FixTypes(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
