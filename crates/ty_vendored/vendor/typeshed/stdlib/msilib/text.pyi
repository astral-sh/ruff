import sys
from typing import Final

if sys.platform == "win32":
    ActionText: Final[list[tuple[str, str, str | None]]]
    UIText: Final[list[tuple[str, str | None]]]
    dirname: str
    tables: Final[list[str]]
