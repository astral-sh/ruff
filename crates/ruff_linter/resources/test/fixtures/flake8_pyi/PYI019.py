from typing import TypeVar, Self, Type

_S = TypeVar("_S", bound=BadClass)
_S2 = TypeVar("_S2", BadClass, GoodClass)

class BadClass:
    def __new__(cls: type[_S], *args: str, **kwargs: int) -> _S: ...  # PYI019


    def bad_instance_method(self: _S, arg: bytes) -> _S: ...  # PYI019


    @classmethod
    def bad_class_method(cls: type[_S], arg: int) -> _S: ...  # PYI019


    @classmethod
    def bad_posonly_class_method(cls: type[_S], /) -> _S: ...  # PYI019


    @classmethod
    def excluded_edge_case(cls: Type[_S], arg: int) -> _S: ...  # Ok


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


# Python > 3.12
class PEP695BadDunderNew[T]:
  def __new__[S](cls: type[S], *args: Any, ** kwargs: Any) -> S: ...  # PYI019


  def generic_instance_method[S](self: S) -> S: ...  # PYI019


class PEP695GoodDunderNew[T]:
  def __new__(cls, *args: Any, **kwargs: Any) -> Self: ...


class CustomClassMethod:
   # Should be recognised as a classmethod decorator
   # due to `foo_classmethod being listed in `pep8_naming.classmethod-decorators`
   # in the settings for this test:
   @foo_classmethod
   def foo[S](cls: type[S]) -> S: ...  # PYI019


# No fixes for .py
class PEP695Fix:
    def __new__[S: PEP695Fix](cls: type[S]) -> S: ...

    def __init_subclass__[S](cls: type[S]) -> S: ...

    def __neg__[S: PEP695Fix](self: S) -> S: ...

    def __pos__[S](self: S) -> S: ...

    def __add__[S: PEP695Fix](self: S, other: S) -> S: ...

    def __sub__[S](self: S, other: S) -> S: ...

    @classmethod
    def class_method_bound[S: PEP695Fix](cls: type[S]) -> S: ...

    @classmethod
    def class_method_unbound[S](cls: type[S]) -> S: ...

    def instance_method_bound[S: PEP695Fix](self: S) -> S: ...

    def instance_method_unbound[S](self: S) -> S: ...

    def instance_method_bound_with_another_parameter[S: PEP695Fix](self: S, other: S) -> S: ...

    def instance_method_unbound_with_another_parameter[S](self: S, other: S) -> S: ...

    def multiple_type_vars[S, *Ts, T](self: S, other: S, /, *args: *Ts, a: T, b: list[T]) -> S: ...
