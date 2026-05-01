# Regressions for correlated constraints

This test exercises several regressions that stem from how our specialization inference does not
always currently combine multiple constraints that we infer when calling a generic function.

## Generic protocol overloads

The generic protocol overload for `Series.mul` can infer multiple correlated specializations for
`(T_contra, S2)`.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Any, Generic, Never, Protocol, TypeVar, overload

T = TypeVar("T")
T_contra = TypeVar("T_contra")
S2 = TypeVar("S2")

class ElementOpsMixin(Generic[S2]):
    @overload
    def _proto_mul(self: "ElementOpsMixin[bool]", other: bool) -> "ElementOpsMixin[bool]": ...
    @overload
    def _proto_mul(self: "ElementOpsMixin[str]", other: str) -> "ElementOpsMixin[str]": ...
    def _proto_mul(self, other):
        raise NotImplementedError

class Supports_ProtoMul(Protocol[T_contra, S2]):
    def _proto_mul(self, other: T_contra, /) -> ElementOpsMixin[S2]: ...

class Series(ElementOpsMixin[T], Generic[T]):
    @overload
    def mul(self: Supports_ProtoMul[T_contra, S2], other: T_contra) -> "Series[S2]": ...
    @overload
    def mul(self: "Series[int]", other: int) -> "Series[int]": ...
    def mul(self, other):
        raise NotImplementedError

    @overload
    def gradual_mul(self: Supports_ProtoMul[T_contra, S2], other: T_contra) -> "Series[S2]": ...
    @overload
    def gradual_mul(self: "Series[Never]", other: object) -> "Series[Any]": ...
    def gradual_mul(self, other):
        raise NotImplementedError

def _(left: Series[bool]):
    # revealed: Series[bool]
    reveal_type(left.mul(True))

def _(left: Series[Any]):
    # revealed: Series[Any]
    reveal_type(left.gradual_mul(True))
```
