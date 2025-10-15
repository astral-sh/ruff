from collections.abc import Generator
from typing import ClassVar, Final, Literal

from .. import fixer_base

MAPPING: Final[dict[str, dict[str, str]]]
LOOKUP: Final[dict[tuple[str, str], str]]

def alternates(members): ...
def build_pattern() -> Generator[str, None, None]: ...

class FixRenames(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    order: ClassVar[Literal["pre"]]
    PATTERN: ClassVar[str]
    def match(self, node): ...
    def transform(self, node, results) -> None: ...
