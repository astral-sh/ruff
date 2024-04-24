from typing import Literal, Any

NewAny = Any
OptionalStr = typing.Optional[str]
Foo = Literal["foo"]
IntOrStr = int | str
AliasNone = None

NewAny: typing.TypeAlias = Any
OptionalStr: TypeAlias = typing.Optional[str]
Foo: typing.TypeAlias = Literal["foo"]
IntOrStr: TypeAlias = int | str
IntOrFloat: Foo = int | float
AliasNone: typing.TypeAlias = None

class NotAnEnum:
    FLAG_THIS = None

# these are ok
from enum import Enum

class FooEnum(Enum): ...

class BarEnum(FooEnum):
    BAR = None

VarAlias = str
AliasFoo = Foo
