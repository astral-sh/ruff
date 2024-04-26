import builtins
import types
import typing
from collections.abc import Awaitable
from types import TracebackType
from typing import Any, Type, overload

import _typeshed
import typing_extensions
from _typeshed import Unused

class GoodOne:
    def __exit__(self, *args: object) -> None: ...
    async def __aexit__(self, *args) -> str: ...

class GoodTwo:
    def __exit__(self, typ: type[builtins.BaseException] | None, *args: builtins.object) -> bool | None: ...
    async def __aexit__(self, /, typ: Type[BaseException] | None, *args: object, **kwargs) -> bool: ...

class GoodThree:
    def __exit__(self, __typ: typing.Type[BaseException] | None, exc: BaseException | None, *args: object) -> None: ...
    async def __aexit__(self, typ: typing_extensions.Type[BaseException] | None, __exc: BaseException | None, *args: object) -> None: ...

class GoodFour:
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> None: ...
    async def __aexit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: types.TracebackType | None, *args: list[None]) -> None: ...

class GoodFive:
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None, weird_extra_arg: int = ..., *args: int, **kwargs: str) -> None: ...
    async def __aexit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> Awaitable[None]: ...

class GoodSix:
    def __exit__(self, typ: object, exc: builtins.object, tb: object) -> None: ...
    async def __aexit__(self, typ: object, exc: object, tb: builtins.object) -> None: ...

class GoodSeven:
    def __exit__(self, *args: Unused) -> bool: ...
    async def __aexit__(self, typ: Type[BaseException] | None, *args: _typeshed.Unused) -> Awaitable[None]: ...

class GoodEight:
    def __exit__(self, __typ: typing.Type[BaseException] | None, exc: BaseException | None, *args: _typeshed.Unused) -> bool: ...
    async def __aexit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None, weird_extra_arg: int = ..., *args: Unused, **kwargs: Unused) -> Awaitable[None]: ...

class GoodNine:
    def __exit__(self, __typ: typing.Union[typing.Type[BaseException] , None], exc: typing.Union[BaseException , None], *args: _typeshed.Unused) -> bool: ...
    async def __aexit__(self, typ: typing.Union[typing.Type[BaseException], None], exc: typing.Union[BaseException , None], tb: typing.Union[TracebackType , None], weird_extra_arg: int = ..., *args: Unused, **kwargs: Unused) -> Awaitable[None]: ...

class GoodTen:
    def __exit__(self, __typ: typing.Optional[typing.Type[BaseException]], exc: typing.Optional[BaseException], *args: _typeshed.Unused) -> bool: ...
    async def __aexit__(self, typ: typing.Optional[typing.Type[BaseException]], exc: typing.Optional[BaseException], tb: typing.Optional[TracebackType], weird_extra_arg: int = ..., *args: Unused, **kwargs: Unused) -> Awaitable[None]: ...


class BadOne:
    def __exit__(self, *args: Any) -> None: ... # PYI036: Bad star-args annotation
    async def __aexit__(self) -> None: ... # PYI036: Missing args

class BadTwo:
    def __exit__(self, typ, exc, tb, weird_extra_arg) -> None: ... # PYI036: Extra arg must have default
    async def __aexit__(self, typ, exc, tb, *, weird_extra_arg1, weird_extra_arg2) -> None: ...# PYI036: kwargs must have default

class BadThree:
    def __exit__(self, typ: type[BaseException], exc: BaseException | None, tb: TracebackType | None) -> None: ... # PYI036: First arg has bad annotation
    async def __aexit__(self, __typ: type[BaseException] | None, __exc: BaseException, __tb: TracebackType) -> bool | None: ... # PYI036: Second arg has bad annotation

class BadFour:
    def __exit__(self, typ: typing.Optional[type[BaseException]], exc: typing.Union[BaseException, None], tb: TracebackType) -> None: ... # PYI036: Third arg has bad annotation
    async def __aexit__(self, __typ: type[BaseException] | None, __exc: BaseException | None, __tb: typing.Union[TracebackType, None, int]) -> bool | None: ... # PYI036: Third arg has bad annotation

class BadFive:
    def __exit__(self, typ: BaseException | None, *args: list[str]) -> bool: ... # PYI036: Bad star-args annotation
    async def __aexit__(self, /, typ: type[BaseException] | None, *args: Any) -> Awaitable[None]: ... # PYI036: Bad star-args annotation

class BadSix:
    def __exit__(self, typ, exc, tb, weird_extra_arg, extra_arg2 = None) -> None: ... # PYI036: Extra arg must have default
    async def __aexit__(self, typ, exc, tb, *, weird_extra_arg) -> None: ... # PYI036: kwargs must have default


def isolated_scope():
    from builtins import type as Type

    class ShouldNotError:
        def __exit__(self, typ: Type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> None: ...

class AllPositionalOnlyArgs:
    def __exit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None, /) -> None: ...
    async def __aexit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None, /) -> None: ...

class BadAllPositionalOnlyArgs:
    def __exit__(self, typ: type[Exception] | None, exc: BaseException | None, tb: TracebackType | None, /) -> None: ...
    async def __aexit__(self, typ: type[BaseException] | None, exc: BaseException | None, tb: TracebackType, /) -> None: ...

# Definitions not in a class scope can do whatever, we don't care
def __exit__(self, *args: bool) -> None: ...
async def __aexit__(self, *, go_crazy: bytes) -> list[str]: ...

# Here come the overloads...

class AcceptableOverload1:
    @overload
    def __exit__(self, exc_typ: None, exc: None, exc_tb: None) -> None: ...
    @overload
    def __exit__(self, exc_typ: type[BaseException], exc: BaseException, exc_tb: TracebackType) -> None: ...

# Using `object` or `Unused` in an overload definition is kinda strange,
# but let's allow it to be on the safe side
class AcceptableOverload2:
    @overload
    def __exit__(self, exc_typ: None, exc: None, exc_tb: object) -> None: ...
    @overload
    def __exit__(self, exc_typ: Unused, exc: BaseException, exc_tb: object) -> None: ...

class AcceptableOverload3:
    # Just ignore any overloads that don't have exactly 3 annotated non-self parameters.
    # We don't have the ability (yet) to do arbitrary checking
    # of whether one function definition is a subtype of another...
    @overload
    def __exit__(self, exc_typ: bool, exc: bool, exc_tb: bool, weird_extra_arg: bool) -> None: ...
    @overload
    def __exit__(self, *args: object) -> None: ...
    @overload
    async def __aexit__(self, exc_typ: bool, /, exc: bool, exc_tb: bool, *, keyword_only: str) -> None: ...
    @overload
    async def __aexit__(self, *args: object) -> None: ...

class AcceptableOverload4:
    # Same as above
    @overload
    def __exit__(self, exc_typ: type[Exception], exc: type[Exception], exc_tb: types.TracebackType) -> None: ...
    @overload
    def __exit__(self, *args: object) -> None: ...
    @overload
    async def __aexit__(self, exc_typ: type[Exception], exc: type[Exception], exc_tb: types.TracebackType, *, extra: str = "foo") -> None: ...
    @overload
    async def __aexit__(self, exc_typ: None, exc: None, tb: None) -> None: ...

class StrangeNumberOfOverloads:
    # Only one overload? Type checkers will emit an error, but we should just ignore it
    @overload
    def __exit__(self, exc_typ: bool, exc: bool, tb: bool) -> None: ...
    # More than two overloads? Anything could be going on; again, just ignore all the overloads
    @overload
    async def __aexit__(self, arg: bool) -> None: ...
    @overload
    async def __aexit__(self, arg: None, arg2: None, arg3: None) -> None: ...
    @overload
    async def __aexit__(self, arg: bool, arg2: bool, arg3: bool) -> None: ...

# TODO: maybe we should emit an error on this one as well?
class BizarreAsyncSyncOverloadMismatch:
    @overload
    def __exit__(self, exc_typ: bool, exc: bool, tb: bool) -> None: ...
    @overload
    async def __exit__(self, exc_typ: bool, exc: bool, tb: bool) -> None: ...

class UnacceptableOverload1:
    @overload
    def __exit__(self, exc_typ: None, exc: None, tb: None) -> None: ...  # Okay
    @overload
    def __exit__(self, exc_typ: Exception, exc: Exception, tb: TracebackType) -> None: ...  # PYI036

class UnacceptableOverload2:
    @overload
    def __exit__(self, exc_typ: type[BaseException] | None, exc: None, tb: None) -> None: ...  # PYI036
    @overload
    def __exit__(self, exc_typ: object, exc: Exception, tb: builtins.TracebackType) -> None: ...  # PYI036
