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


_S695 = TypeVar("_S695", bound="PEP695Fix")


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

    def mixing_old_and_new_style_type_vars[T](self: _S695, a: T, b: T) -> _S695: ...


class InvalidButWeDoNotPanic:
    @classmethod
    def m[S](cls: type[S], /) -> S[int]: ...
    def n(self: S) -> S[int]: ...


import builtins

class UsesFullyQualifiedType:
    @classmethod
    def m[S](cls: builtins.type[S]) -> S: ...  # False negative (#15821)


def shadowed_type():
    type = 1
    class A:
        @classmethod
        def m[S](cls: type[S]) -> S: ...  # no error here


class SubscriptReturnType:
    @classmethod
    def m[S](cls: type[S]) -> type[S]: ...  # PYI019


class PEP695TypeParameterAtTheVeryEndOfTheList:
    def f[T, S](self: S) -> S: ...


class PEP695Again:
    def mixing_and_nested[T](self: _S695, a: list[_S695], b: dict[_S695, str | T | set[_S695]]) -> _S695: ...
    def also_uses_s695_but_should_not_be_edited(self, v: set[tuple[_S695]]) -> _S695: ...

    @classmethod
    def comment_in_fix_range[T, S](
        cls: type[  # Lorem ipsum
            S
        ],
        a: T,
        b: tuple[S, T]
    ) -> S: ...

    def comment_outside_fix_range[T, S](
        self: S,
        a: T,
        b: tuple[
            # Lorem ipsum
            S, T
        ]
    ) -> S: ...


class SelfNotUsedInReturnAnnotation:
    def m[S](self: S, other: S) -> int: ...
    @classmethod
    def n[S](cls: type[S], other: S) -> int: ...


class _NotATypeVar: ...

# Our stable-mode logic uses heuristics and thinks this is a `TypeVar`
# because `self` and the return annotation use the same name as their annotation,
# but our preview-mode logic is smarter about this.
class Foo:
    def x(self: _NotATypeVar) -> _NotATypeVar: ...
    @classmethod
    def y(self: type[_NotATypeVar]) -> _NotATypeVar: ...

class NoReturnAnnotations:
    def m[S](self: S, other: S): ...
    @classmethod
    def n[S](cls: type[S], other: S): ...

class MultipleBoundParameters:
    def m[S: int, T: int](self: S, other: T) -> S: ...
    def n[T: (int, str), S: (int, str)](self: S, other: T) -> S: ...


MetaType = TypeVar("MetaType")

class MetaTestClass(type):
    def m(cls: MetaType) -> MetaType:
        return cls
