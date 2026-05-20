from collections.abc import Iterator
from types import GenericAlias
from typing import Any, Generic, Literal, TypeVar, final, overload

_T = TypeVar("_T")

@final
class Template:  # TODO: consider making `Template` generic on `TypeVarTuple`
    strings: tuple[str, ...]
    interpolations: tuple[Interpolation[Any], ...]

    def __new__(cls, *args: str | Interpolation[Any]) -> Template: ...
    def __iter__(self) -> Iterator[str | Interpolation[Any]]: ...
    def __add__(self, other: Template, /) -> Template: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...
    @property
    def values(self) -> tuple[Any, ...]: ...  # Tuple of interpolation values, which can have any type

@final
class Interpolation(Generic[_T]):
    value: _T
    expression: str
    conversion: Literal["a", "r", "s"] | None
    format_spec: str

    __match_args__ = ("value", "expression", "conversion", "format_spec")

    def __new__(
        cls, value: _T, expression: str = "", conversion: Literal["a", "r", "s"] | None = None, format_spec: str = ""
    ) -> Interpolation[_T]: ...
    def __class_getitem__(cls, item: Any, /) -> GenericAlias: ...

@overload
def convert(obj: _T, /, conversion: None) -> _T: ...
@overload
def convert(obj: object, /, conversion: Literal["r", "s", "a"]) -> str: ...
