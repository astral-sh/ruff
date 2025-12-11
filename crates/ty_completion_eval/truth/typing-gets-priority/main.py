# We should prefer `typing` over `asyncio` here.
class Foo(Protoco<CURSOR: typing.Protocol>): ...

# We should prefer `typing` over `ty_extensions`
# or `typing_extensions`.
reveal_<CURSOR: typing.reveal_type>

# We should prefer `typing` over `ast`.
TypeVa<CURSOR: typing.TypeVar>

# We should prefer `typing` over `ctypes`.
cast<CURSOR: typing.cast>
