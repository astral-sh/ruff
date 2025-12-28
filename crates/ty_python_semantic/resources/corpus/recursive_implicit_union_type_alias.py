# Regression test for https://github.com/astral-sh/ty/issues/1998
# Checking this code was previously very slow or would hang.

from __future__ import annotations

from typing import Protocol, Union

class Traceable(Protocol):
    def trace_repr(self) -> TraceableValue:
        ...

TraceableValue = Union[
    Traceable,
    bool,
    tuple["TraceableValue", ...],
]

class FilledOutfit:
    def trace_repr(self) -> TraceableValue:
        return False
