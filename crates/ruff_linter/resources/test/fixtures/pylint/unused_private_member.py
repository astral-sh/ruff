# Test cases for PLW0238 (unused-private-member)
#
# Note: This rule detects unused private class-level bindings (methods and class variables).
# Due to how Python's semantic model works, references via `self.__method()` or `self.__var`
# are attribute accesses, not references to the class-level binding. The rule currently
# flags these as unused because the class-level binding itself has no direct references.

# Case 1: Unused private method (should flag)
class UnusedMethod:
    def __unused(self):  # PLW0238
        pass


# Case 2: Dunder methods like __init__ (should NOT flag)
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


# Case 4: Multiple unused members in same class (should flag all)
class MultipleUnused:
    __unused1 = 1  # PLW0238
    __unused2 = 2  # PLW0238

    def __unused_method1(self):  # PLW0238
        pass

    def __unused_method2(self):  # PLW0238
        pass


# Case 5: Edge case - name with trailing underscore (should flag if unused)
class TrailingUnderscore:
    __private_ = 1  # PLW0238 - unused (not a dunder)


# Case 6: Private static method (should flag if unused)
class PrivateStaticMethod:
    @staticmethod
    def __unused_static():  # PLW0238
        pass


# Case 7: Private class method (should flag if unused)
class PrivateClassMethod:
    @classmethod
    def __unused_classmethod(cls):  # PLW0238
        pass


# Case 8: Private method with decorator (should flag if unused)
class PrivateDecorated:
    def some_decorator(func):
        return func

    @some_decorator
    def __decorated_unused(self):  # PLW0238
        pass


# Case 9: Multiple classes - isolation test
class ClassA:
    __private_a = 1  # PLW0238 - unused


class ClassB:
    __private_b = 2  # PLW0238 - unused


# Case 10: Nested class with unused private member
class OuterWithNested:
    __outer_unused = 1  # PLW0238

    class Inner:
        __inner_unused = 2  # PLW0238


# Case 11: Private method that calls another private method (both unused at class level)
class PrivateCallingPrivate:
    def __helper(self):  # PLW0238
        return 1

    def __main_private(self):  # PLW0238
        return self.__helper()


# Case 12: Property descriptor usage (should NOT flag - property() creates reference)
class PropertyExample:
    def __init__(self):
        self._value = 0

    def __get_value(self):  # OK - referenced by property()
        return self._value

    def __set_value(self, val):  # OK - referenced by property()
        self._value = val

    value = property(__get_value, __set_value)


# The following cases demonstrate current limitations where the rule flags
# members that ARE used via self/cls access, but the semantic model doesn't
# track these as references to the class-level binding:

# Case 13: Used private method via self (currently flagged - known limitation)
class UsedMethodViaSelf:
    def __used(self):  # PLW0238 (limitation: self.__used() not tracked as reference)
        pass

    def public(self):
        self.__used()


# Case 14: Used private class variable via self (currently flagged - known limitation)
class UsedClassVarViaSelf:
    __used_var = 42  # PLW0238 (limitation: self.__used_var not tracked as reference)

    def get_var(self):
        return self.__used_var


# Case 15: Name-mangled access from outside class (currently flagged - known limitation)
class MangledAccess:
    __private = 1  # PLW0238 (limitation: mangled access not tracked)


obj = MangledAccess()
print(obj._MangledAccess__private)


# ============================================================================
# Additional test cases for edge cases and special scenarios
# ============================================================================

# Case 16: Abstract method (should flag if unused - abstract doesn't exempt from usage)
from abc import ABC, abstractmethod


class AbstractExample(ABC):
    @abstractmethod
    def __abstract_unused(self):  # PLW0238 - abstract but still unused
        pass


# Case 17: Multiple inheritance
class Base1:
    __base1_unused = 1  # PLW0238


class Base2:
    __base2_unused = 2  # PLW0238


class MultiInherit(Base1, Base2):
    __multi_unused = 3  # PLW0238


# Case 18: __slots__ interaction - __slots__ itself is NOT a private member
class WithSlots:
    __slots__ = ("__private_slot",)  # OK - __slots__ is a dunder
    __unused_in_slots_class = 1  # PLW0238


# Case 19: Private method with multiple decorators
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


# Case 21: Private generator method
class GeneratorExample:
    def __generator_unused(self):  # PLW0238
        yield 1


# Case 22: Private method with *args and **kwargs
class ArgsKwargsExample:
    def __complex_signature(self, *args, **kwargs):  # PLW0238
        pass


# Case 23: Class with only dunder methods (should NOT flag anything)
class OnlyDunders:
    def __init__(self):
        pass

    def __repr__(self):
        return "OnlyDunders()"

    def __hash__(self):
        return 0


# Case 24: Private class variable with type annotation
class TypeAnnotated:
    __typed_unused: int = 42  # PLW0238


# Case 25: Private method returning complex type
class ComplexReturn:
    def __returns_dict(self) -> dict[str, int]:  # PLW0238
        return {}


# Case 26: Dataclass with private class variable
from dataclasses import dataclass


@dataclass
class DataclassExample:
    __class_var_unused = "default"  # PLW0238
    public_field: str = "value"


# Case 27: Enum-like class with private members
class EnumLike:
    __PRIVATE_CONST = 100  # PLW0238
    PUBLIC_CONST = 200


# Case 28: Private method that's a lambda assigned to class variable
class LambdaExample:
    __lambda_unused = lambda self: 42  # PLW0238


# Case 29: Deeply nested class
class Level1:
    __level1_unused = 1  # PLW0238

    class Level2:
        __level2_unused = 2  # PLW0238

        class Level3:
            __level3_unused = 3  # PLW0238


# Case 30: Private method with default argument
class DefaultArg:
    def __with_default(self, x=10):  # PLW0238
        return x


# Case 31: Private method that shadows builtin
class ShadowsBuiltin:
    def __len(self):  # PLW0238 - not __len__, so it's private
        return 0


# Case 32: Single underscore prefix (should NOT flag - not private)
class SingleUnderscore:
    _protected = 1  # OK - single underscore is "protected", not "private"

    def _protected_method(self):  # OK
        pass


# Case 33: Triple underscore (edge case - still private, not dunder)
class TripleUnderscore:
    ___triple = 1  # PLW0238 - starts with __ but doesn't end with __


# Case 34: @property decorator pattern with setter (should NOT flag - setter references it)
class PropertyDecorator:
    def __init__(self):
        self._value = 0

    @property
    def __private_prop(self):  # OK - referenced by @__private_prop.setter below
        return self._value

    @__private_prop.setter
    def __private_prop(self, val):  # OK - setter references the property
        self._value = val


# Case 34b: @property without setter (should flag if unused)
class PropertyNoSetter:
    @property
    def __unused_prop(self):  # PLW0238 - unused property (no setter referencing it)
        return 42


# Case 35: functools.cached_property (should flag if unused)
from functools import cached_property


class CachedPropertyExample:
    @cached_property
    def __cached_unused(self):  # PLW0238 - unused cached property
        return 42


# Case 36: __all__ interaction - private members in __all__ (edge case)
# Note: Including private members in __all__ is unusual but valid Python
class WithAllExport:
    __exported_private = 1  # PLW0238 - still flagged (class-level binding unused)


__all__ = ["WithAllExport"]  # __all__ at module level doesn't affect class members


# Case 37: Private method used via cls in classmethod (known limitation)
class UsedViaCls:
    __class_var = 100  # PLW0238 (limitation: cls.__class_var not tracked)

    @classmethod
    def get_var(cls):
        return cls.__class_var


# Case 38: Private method referenced in class body (should NOT flag)
class ReferencedInBody:
    def __helper(self):  # OK - referenced by method_ref
        return 1

    method_ref = __helper  # Direct reference in class body
