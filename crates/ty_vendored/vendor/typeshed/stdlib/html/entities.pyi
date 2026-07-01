from typing import Final

__all__ = ["html5", "name2codepoint", "codepoint2name", "entitydefs"]

name2codepoint: Final[dict[str, int]]
html5: Final[dict[str, str]]
codepoint2name: Final[dict[int, str]]
entitydefs: Final[dict[str, str]]
