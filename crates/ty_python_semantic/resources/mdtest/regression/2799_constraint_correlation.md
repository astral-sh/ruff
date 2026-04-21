# Regressions for correlated constraints

This test exercises several regressions that stem from how our specialization inference does not
always currently combine multiple constraints that we infer when calling a generic function.

## Generic protocol overloads

The generic protocol overload for `Series.mul` can infer multiple correlated specializations for
`(T_contra, S2)`.

TODO: We currently collapse those disjunctive solutions into independent unions in
`SpecializationBuilder.types`, which can produce an impossible pair and reject the overload. This
should be fixed once we are using a constraint set for our internal state.

```toml
[environment]
python-version = "3.13"
```

```py
from typing import Generic, Protocol, TypeVar, overload

T = TypeVar("T")
T_contra = TypeVar("T_contra")
S2 = TypeVar("S2")

class ElementOpsMixin(Generic[S2]):
    @overload
    def _proto_mul(self, other: bool) -> "ElementOpsMixin[bool]": ...
    @overload
    def _proto_mul(self, other: str) -> "ElementOpsMixin[str]": ...
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

def _(left: Series[bool]):
    # TODO: no error
    # TODO: revealed: Series[bool]
    # error: [no-matching-overload]
    # revealed: Unknown
    reveal_type(left.mul(True))
```
