from typing import TypeVar, Self

_S = TypeVar("_S", bound=BadClass)
_S2 = TypeVar("_S2", BadClass, GoodClass)

class BadClass:
    def __new__(cls: type[_S], *args: str, **kwargs: int) -> _S: ...  # Ok
    def bad_instance_method(self: _S, arg: bytes) -> _S: ...  # Ok
    @classmethod
    def bad_class_method(cls: type[_S], arg: int) -> _S: ...  # Ok

class GoodClass:
    def __new__(cls: type[Self], *args: list[int], **kwargs: set[str]) -> Self: ...
    def good_instance_method_1(self: Self, arg: bytes) -> Self: ...
    def good_instance_method_2(self, arg1: _S2, arg2: _S2) -> _S2: ...
    @classmethod
    def good_cls_method_1(cls: type[Self], arg: int) -> Self: ...
    @classmethod
    def good_cls_method_2(cls, arg1: _S, arg2: _S) -> _S: ...
    @staticmethod
    def static_method(arg1: _S) -> _S: ...
