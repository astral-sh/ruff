"""Classes that declare their type parameters with a `Protocol[...]` base.

These are only flagged in preview.
"""

import typing
from typing import Generic, Protocol, TypeVar

from somewhere import SupportsRichComparisonT

T = TypeVar("T")
S = TypeVar("S")


class Basic(Protocol[T]):
    def method(self) -> T: ...


class Qualified(typing.Protocol[T]): ...


class Multiple(Protocol[T, S]): ...


class OtherProtocol(Protocol): ...


class OtherBaseFirst(OtherProtocol, Protocol[T]): ...


class Parenthesized((Protocol)[T]): ...


# Diagnostic without a fix: `Protocol[T]` is not the last base.
class NotLast(Protocol[T], OtherProtocol): ...


# Diagnostic without a fix: the type parameter can't be resolved to a `TypeVar`.
class ExternalType(Protocol[SupportsRichComparisonT]): ...


# Diagnostic without a fix: `int` is not a type variable.
class NotATypeVar(Protocol[int]): ...


# Outside of preview, `Generic[T]` is found and fixed. In preview, the search stops at the first
# matching base, `Protocol[T]`, which is not the last base, so there is no fix.
class ProtocolAndGeneric(Protocol[T], Generic[T]): ...


### No errors

# Duplicate type parameters.
class Duplicated(Protocol[T, T]): ...


# `Protocol` without a subscript does not declare any type parameters.
class NoSubscript(Protocol): ...
