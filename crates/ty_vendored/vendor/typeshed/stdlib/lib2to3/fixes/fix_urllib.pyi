"""Fix changes imports of urllib which are now incompatible.
This is rather similar to fix_imports, but because of the more
complex nature of the fixing for urllib, it has its own fixer.
"""

from collections.abc import Generator
from typing import Final, Literal

from .fix_imports import FixImports

MAPPING: Final[dict[str, list[tuple[Literal["urllib.request", "urllib.parse", "urllib.error"], list[str]]]]]

def build_pattern() -> Generator[str, None, None]: ...

class FixUrllib(FixImports):
    def build_pattern(self): ...
    def transform_import(self, node, results) -> None:
        """Transform for the basic import case. Replaces the old
        import name with a comma separated list of its
        replacements.
        """

    def transform_member(self, node, results):
        """Transform for imports of specific module elements. Replaces
        the module to be imported from with the appropriate new
        module.
        """

    def transform_dot(self, node, results) -> None:
        """Transform for calls to module members in code."""

    def transform(self, node, results) -> None: ...
