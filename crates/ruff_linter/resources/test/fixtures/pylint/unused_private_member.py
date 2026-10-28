# ruff: noqa: E501
"""Test cases for PLW0238 (unused-private-member)."""
from abc import ABC, abstractmethod
from dataclasses import dataclass
from functools import cached_property


# Case 1: Unused private method (should flag)
class UnusedMethod:
    def __unused(self):  # PLW0238
        pass


# Case 2: Dunder methods (should NOT flag)
class DunderMethod:
    def __init__(self):
        pass

    def __str__(self):
        return "DunderMethod"

    def __eq__(self, other):
        return True


# Case 3: Unused private class variable (should flag)
class UnusedClassVar:
    __unused_var = 42  # PLW0238


# Case 4: Multiple unused members (should flag all)
class MultipleUnused:
    __unused1 = 1  # PLW0238
    __unused2 = 2  # PLW0238

    def __unused_method1(self):  # PLW0238
        pass

    def __unused_method2(self):  # PLW0238
        pass


# Case 5: Trailing underscore (should flag)
class TrailingUnderscore:
    __private_ = 1  # PLW0238


# Case 6: Private static method (should flag)
class PrivateStaticMethod:
    @staticmethod
    def __unused_static():  # PLW0238
        pass


# Case 7: Private class method (should flag)
class PrivateClassMethod:
    @classmethod
    def __unused_classmethod(cls):  # PLW0238
        pass


# Case 8: Private method with decorator (should flag)
class PrivateDecorated:
    def some_decorator(func):
        return func

    @some_decorator
    def __decorated_unused(self):  # PLW0238
        pass


# Case 9: Multiple classes - isolation test
class ClassA:
    __private_a = 1  # PLW0238


class ClassB:
    __private_b = 2  # PLW0238


# Case 10: Nested class
class OuterWithNested:
    __outer_unused = 1  # PLW0238

    class Inner:
        __inner_unused = 2  # PLW0238


# Case 11: Private method calling another private method
class PrivateCallingPrivate:
    def __helper(self):  # OK - used via self.__helper()
        return 1

    def __main_private(self):  # PLW0238
        return self.__helper()


# Case 12: Property descriptor usage (should NOT flag)
class PropertyExample:
    def __init__(self):
        self._value = 0

    def __get_value(self):  # OK - referenced by property()
        return self._value

    def __set_value(self, val):  # OK - referenced by property()
        self._value = val

    value = property(__get_value, __set_value)


# Case 13: Used via self (should NOT flag)
class UsedMethodViaSelf:
    def __used(self):  # OK
        pass

    def public(self):
        self.__used()


# Case 14: Used class variable via self (should NOT flag)
class UsedClassVarViaSelf:
    __used_var = 42  # OK

    def get_var(self):
        return self.__used_var


# Case 15: External mangled access (known limitation)
class MangledAccess:
    __private = 1  # PLW0238


obj = MangledAccess()
print(obj._MangledAccess__private)


# Case 16: Abstract method (should flag)
class AbstractExample(ABC):
    @abstractmethod
    def __abstract_unused(self):  # PLW0238
        pass


# Case 17: Multiple inheritance
class Base1:
    __base1_unused = 1  # PLW0238


class Base2:
    __base2_unused = 2  # PLW0238


class MultiInherit(Base1, Base2):
    __multi_unused = 3  # PLW0238


# Case 18: __slots__ interaction
class WithSlots:
    __slots__ = ("__private_slot",)  # OK - dunder
    __unused_in_slots_class = 1  # PLW0238


# Case 19: Multiple decorators
class MultiDecorated:
    def decorator1(func):
        return func

    def decorator2(func):
        return func

    @decorator1
    @decorator2
    def __multi_decorated_unused(self):  # PLW0238
        pass


# Case 20: Private async method
class AsyncExample:
    async def __async_unused(self):  # PLW0238
        pass


# Case 21: Private generator
class GeneratorExample:
    def __generator_unused(self):  # PLW0238
        yield 1


# Case 22: *args and **kwargs
class ArgsKwargsExample:
    def __complex_signature(self, *args, **kwargs):  # PLW0238
        pass


# Case 23: Only dunders (should NOT flag)
class OnlyDunders:
    def __init__(self):
        pass

    def __repr__(self):
        return "OnlyDunders()"

    def __hash__(self):
        return 0


# Case 24: Type annotation
class TypeAnnotated:
    __typed_unused: int = 42  # PLW0238


# Case 25: Complex return type
class ComplexReturn:
    def __returns_dict(self) -> dict[str, int]:  # PLW0238
        return {}


# Case 26: Dataclass
@dataclass
class DataclassExample:
    __class_var_unused = "default"  # PLW0238
    public_field: str = "value"


# Case 27: Enum-like
class EnumLike:
    __PRIVATE_CONST = 100  # PLW0238
    PUBLIC_CONST = 200


# Case 28: Lambda assignment
class LambdaExample:
    __lambda_unused = lambda self: 42  # noqa: E731  # PLW0238


# Case 29: Deeply nested
class Level1:
    __level1_unused = 1  # PLW0238

    class Level2:
        __level2_unused = 2  # PLW0238

        class Level3:
            __level3_unused = 3  # PLW0238


# Case 30: Default argument
class DefaultArg:
    def __with_default(self, x=10):  # PLW0238
        return x


# Case 31: Shadows builtin
class ShadowsBuiltin:
    def __len(self):  # PLW0238
        return 0


# Case 32: Single underscore (should NOT flag)
class SingleUnderscore:
    _protected = 1  # OK

    def _protected_method(self):  # OK
        pass


# Case 33: Triple underscore
class TripleUnderscore:
    ___triple = 1  # PLW0238


# Case 34: @property with setter (should NOT flag)
class PropertyDecorator:
    def __init__(self):
        self._value = 0

    @property
    def __private_prop(self):  # OK
        return self._value

    @__private_prop.setter
    def __private_prop(self, val):  # OK
        self._value = val


# Case 34b: @property without setter
class PropertyNoSetter:
    @property
    def __unused_prop(self):  # PLW0238
        return 42


# Case 35: cached_property
class CachedPropertyExample:
    @cached_property
    def __cached_unused(self):  # PLW0238
        return 42


# Case 36: __all__ interaction
class WithAllExport:
    __exported_private = 1  # PLW0238


__all__ = ["WithAllExport"]


# Case 37: Used via cls (should NOT flag)
class UsedViaCls:
    __class_var = 100  # OK

    @classmethod
    def get_var(cls):
        return cls.__class_var


# Case 38: Referenced in class body (should NOT flag)
class ReferencedInBody:
    def __helper(self):  # OK
        return 1

    method_ref = __helper


# Case 39: Used via type(self) (should NOT flag)
class UsedViaType:
    __class_attr = 42  # OK

    def get_attr(self):
        return type(self).__class_attr


# Case 40: Used via super() in child class (known limitation - parent flagged)
# The rule analyzes each class independently, so super().__x in child
# doesn't mark the parent's private member as used.
class ParentClass:
    def __parent_method(self):  # PLW0238 - flagged (known limitation)
        return "parent"


class ChildClass(ParentClass):
    def call_parent(self):
        return super().__parent_method()


# Case 41: Used via ClassName (should NOT flag)
class UsedViaClassName:
    __class_const = 100  # OK

    def get_const(self):
        return UsedViaClassName.__class_const


# Case 42: super().__x in subclass (known limitation - parent flagged)
# Same as Case 40 - cross-class analysis not performed.
class ParentWithPrivate:
    def __parent_method(self):  # PLW0238 - flagged (known limitation)
        return "parent"


class ChildUsingSuper(ParentWithPrivate):
    def call_parent(self):
        return super().__parent_method()


# Case 43: cls.__private_method() in classmethod (should NOT flag)
class UsedViaClsMethod:
    def __private_method(self):  # OK - used via cls.__private_method
        return "private"

    @classmethod
    def call_private(cls):
        instance = cls()
        return cls.__private_method(instance)


# Case 44: Chained attribute access (should flag - different object's private)
class ChainedAccess:
    __own_private = 1  # PLW0238 - not accessed via self

    def __init__(self, other):
        self.other = other

    def get_other_private(self):
        # This accesses other's __private, not our own __own_private
        return self.other.__their_private


class OtherClass:
    __their_private = 2  # PLW0238 - external access not tracked


# Case 45: Walrus operator with private member (should NOT flag)
class WalrusOperator:
    __private_value = 42  # OK - used via walrus operator

    def check_value(self):
        if (val := self.__private_value) > 0:
            return val
        return 0


# Case 46: super(ClassName, self).__x explicit form - cross-class (known limitation)
# Same as Cases 40/42 - cross-class analysis not performed.
# The explicit super() form IS tracked, but only within the same class.
class ExplicitSuperParent:
    def __parent_method(self):  # PLW0238 - flagged (cross-class limitation)
        return "parent"


class ExplicitSuperChild(ExplicitSuperParent):
    def call_parent(self):
        return super(ExplicitSuperChild, self).__parent_method()


# Case 47: Aliased self (should NOT flag - now tracked)
class AliasedSelf:
    __private_via_alias = 1  # OK - aliased self is now tracked

    def use_via_alias(self):
        s = self
        return s.__private_via_alias  # This access is now tracked


# Case 48: Aliased cls (should NOT flag - now tracked)
class AliasedCls:
    __class_var = 100  # OK - aliased cls is now tracked

    @classmethod
    def use_via_alias(cls):
        c = cls
        return c.__class_var  # This access is now tracked


# Case 49: super(ClassName, self).__x explicit form - same class (should NOT flag)
class ExplicitSuperSameClass:
    def __private_method(self):  # OK - used via explicit super() in same class
        return "private"

    def call_via_explicit_super(self):
        return super(ExplicitSuperSameClass, self).__private_method()
