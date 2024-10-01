from typing import ClassVar, Sequence, Final


class A:
    __slots__ = {
        "mutable_default": "A mutable default value",
    }

    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    class_variable: ClassVar[list[int]] = []
    final_variable: Final[list[int]] = []
    class_variable_without_subscript: ClassVar = []
    final_variable_without_subscript: Final = []


from dataclasses import dataclass, field


@dataclass
class C:
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    perfectly_fine: list[int] = field(default_factory=list)
    class_variable: ClassVar[list[int]] = []
    final_variable: Final[list[int]] = []


from pydantic import BaseModel


class D(BaseModel):
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    class_variable: ClassVar[list[int]] = []
    final_variable: Final[list[int]] = []


from msgspec import Struct


class E(Struct):
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    class_variable: ClassVar[list[int]] = []
    final_variable: Final[list[int]] = []


from pydantic_settings import BaseSettings


class F(BaseSettings):
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    class_variable: ClassVar[list[int]] = []
    final_variable: Final[list[int]] = []


class G(F):
    mutable_default: list[int] = []
    immutable_annotation: Sequence[int] = []
    without_annotation = []
    class_variable: ClassVar[list[int]] = []
    final_variable: Final[list[int]] = []


from pydantic import BaseConfig


class H(BaseModel):
    class Config(BaseConfig):
        mutable_default: list[int] = []
        immutable_annotation: Sequence[int] = []
        without_annotation = []
        class_variable: ClassVar[list[int]] = []
        final_variable: Final[list[int]] = []
