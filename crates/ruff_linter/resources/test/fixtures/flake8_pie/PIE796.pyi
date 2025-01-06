import enum


class FakeEnum1(enum.Enum):
    A = ...
    B = ...
    C = ...


from typing import cast

class FakeEnum2(enum.Enum):
    A = cast(SomeType, ...)
    B = cast(SomeType, ...)
    C = cast(SomeType, ...)
