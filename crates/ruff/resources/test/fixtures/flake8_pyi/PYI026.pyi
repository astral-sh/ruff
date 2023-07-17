import typing
from typing import TypeAlias, Literal, Any

NewAny = Any
OptinalStr = typing.Optional[str]
Foo = Literal["foo"]
IntOrStr = int | str
AliasNone = None

NewAny: typing.TypeAlias = Any
OptinalStr: TypeAlias = typing.Optional[str]
Foo: typing.TypeAlias = Literal["foo"]
IntOrStr: TypeAlias = int | str
AliasNone: typing.TypeAlias = None

# these are ok
VarAlias = str
AliasFoo = Foo
