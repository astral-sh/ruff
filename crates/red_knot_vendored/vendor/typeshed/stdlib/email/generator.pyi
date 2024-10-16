from _typeshed import SupportsWrite
from email.message import Message
from email.policy import Policy
from typing import Any, Generic, TypeVar, overload
from typing_extensions import Self

__all__ = ["Generator", "DecodedGenerator", "BytesGenerator"]

# By default, generators do not have a message policy.
_MessageT = TypeVar("_MessageT", bound=Message, default=Any)

class Generator(Generic[_MessageT]):
    maxheaderlen: int | None
    policy: Policy[_MessageT] | None
    @overload
    def __init__(
        self: Generator[Any],  # The Policy of the message is used.
        outfp: SupportsWrite[str],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        *,
        policy: None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        outfp: SupportsWrite[str],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        *,
        policy: Policy[_MessageT],
    ) -> None: ...
    def write(self, s: str) -> None: ...
    def flatten(self, msg: _MessageT, unixfrom: bool = False, linesep: str | None = None) -> None: ...
    def clone(self, fp: SupportsWrite[str]) -> Self: ...

class BytesGenerator(Generator[_MessageT]):
    @overload
    def __init__(
        self: BytesGenerator[Any],  # The Policy of the message is used.
        outfp: SupportsWrite[bytes],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        *,
        policy: None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        outfp: SupportsWrite[bytes],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        *,
        policy: Policy[_MessageT],
    ) -> None: ...

class DecodedGenerator(Generator[_MessageT]):
    @overload
    def __init__(
        self: DecodedGenerator[Any],  # The Policy of the message is used.
        outfp: SupportsWrite[str],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        fmt: str | None = None,
        *,
        policy: None = None,
    ) -> None: ...
    @overload
    def __init__(
        self,
        outfp: SupportsWrite[str],
        mangle_from_: bool | None = None,
        maxheaderlen: int | None = None,
        fmt: str | None = None,
        *,
        policy: Policy[_MessageT],
    ) -> None: ...
