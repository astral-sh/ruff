from typing import Final

from . import fix_imports

MAPPING: Final[dict[str, str]]

class FixImports2(fix_imports.FixImports):
    mapping = MAPPING
