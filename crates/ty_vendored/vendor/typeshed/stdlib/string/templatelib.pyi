"""Support for template string literals (t-strings)."""

from collections.abc import Iterator
from types import GenericAlias
from typing import Any, Literal, TypeVar, final, overload

_T = TypeVar("_T")

@final
class Template:  # TODO: consider making `Template` generic on `TypeVarTuple`
    """Template object"""

    strings: tuple[str, ...]
    interpolations: tuple[Interpolation, ...]

    def __new__(cls, *args: str | Interpolation) -> Template: ...
    def __iter__(self) -> Iterator[str | Interpolation]:
        """Implement iter(self)."""

    def __add__(self, other: Template, /) -> Template:
        """Return self+value."""

    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

    @property
    def values(self) -> tuple[Any, ...]:  # Tuple of interpolation values, which can have any type
        """Values of interpolations"""

@final
class Interpolation:
    """Interpolation object"""

    value: Any  # TODO: consider making `Interpolation` generic in runtime
    expression: str
    conversion: Literal["a", "r", "s"] | None
    format_spec: str

    __match_args__ = ("value", "expression", "conversion", "format_spec")

    def __new__(
        cls, value: Any, expression: str = "", conversion: Literal["a", "r", "s"] | None = None, format_spec: str = ""
    ) -> Interpolation: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias:
        """See PEP 585"""

@overload
def convert(obj: _T, /, conversion: None) -> _T:
    """Convert *obj* using formatted string literal semantics."""

@overload
def convert(obj: object, /, conversion: Literal["r", "s", "a"]) -> str: ...
