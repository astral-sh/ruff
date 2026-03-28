class Parent:
    def method(self):
        pass

    def wrong(self):
        pass


class Child(Parent):
    def method(self):
        parent = super()  # ok
        super().method()  # ok
        Parent.method(self)  # ok
        Parent.super(1, 2)  # ok

    def wrong(self):
        parent = super(Child, self)  # wrong
        super(Child, self).method  # wrong
        super(
            Child,
            self,
        ).method()  # wrong


class BaseClass:
    def f(self):
        print("f")


def defined_outside(self):
    super(MyClass, self).f()  # CANNOT use super()


class MyClass(BaseClass):
    def normal(self):
        super(MyClass, self).f()  # can use super()
        super().f()

    def different_class(self):
        super(BaseClass, self).f()  # CANNOT use super()

    def different_argument(self, other):
        super(MyClass, other).f()  # CANNOT use super()

    def comprehension_scope(self):
        [super(MyClass, self).f() for x in [1]]  # CANNOT use super()

    def comprehension_scope_set(self):
        {super(MyClass, self).f() for x in [1]}  # CANNOT use super()

    def comprehension_scope_generator(self):
        (super(MyClass, self).f() for x in [1])  # CANNOT use super()

    def comprehension_scope_dict(self):
        {True: super(MyClass, self).f() for x in [1]}  # CANNOT use super()

    def nested_comprehension_scope(self):
        [
            (
                [x for x in [1]],
                super(MyClass, self).f(),
            )
            for _ in [1]
        ]  # CANNOT use super()

    def inner_functions(self):
        def outer_argument():
            super(MyClass, self).f()  # CANNOT use super()

        def inner_argument(self):
            super(MyClass, self).f()  # can use super()
            super().f()

        outer_argument()
        inner_argument(self)

    def lambda_closure(self):
        g = lambda: super(MyClass, self).f()  # CANNOT use super()
        g()

    def inner_class(self):
        class InnerClass:
            super(MyClass, self).f()  # CANNOT use super()

            def method(inner_self):
                super(MyClass, self).f()  # CANNOT use super()

        InnerClass().method()

    defined_outside = defined_outside

from dataclasses import dataclass


@dataclass
class DataClass:
    def normal(self):
        super(DataClass, self).f()  # Error
        super().f()  # OK


@dataclass(slots=True)
def normal(self):
    super(DataClass, self).f()  # OK
    super().f()  # OK (`TypeError` in practice)


# see: https://github.com/astral-sh/ruff/issues/18477
class A:
    def foo(self):
        pass


class B(A):
    def bar(self):
        super(__class__, self).foo()


# see: https://github.com/astral-sh/ruff/issues/18684
class C:
    def f(self):
        super = print
        super(C, self)


import builtins


class C:
    def f(self):
        builtins.super(C, self)


# see: https://github.com/astral-sh/ruff/issues/18533
class ClassForCommentEnthusiasts(BaseClass):
    def with_comments(self):
        super(
            # super helpful comment
            ClassForCommentEnthusiasts,
            self
        ).f()
        super(
            ClassForCommentEnthusiasts,
            # even more helpful comment
            self
        ).f()
        super(
            ClassForCommentEnthusiasts,
            self
            # also a comment
        ).f()


# Issue #19096: super calls with keyword arguments should emit diagnostic but not be fixed
class Ord(int):
    def __len__(self):
        return super(Ord, self, uhoh=True, **{"error": True}).bit_length()

class ExampleWithKeywords:
    def method1(self):
        super(ExampleWithKeywords, self, invalid=True).some_method()  # Should emit diagnostic but NOT be fixed
    
    def method2(self):
        super(ExampleWithKeywords, self, **{"kwarg": "value"}).some_method()  # Should emit diagnostic but NOT be fixed
    
    def method3(self):
        super(ExampleWithKeywords, self).some_method()  # Should be fixed - no keywords

# See: https://github.com/astral-sh/ruff/issues/19357
# Must be detected
class ParentD:
    def f(self):
        print("D")

class ChildD1(ParentD):
    def f(self):
        if False: __class__ # Python injects __class__ into scope
        builtins.super(ChildD1, self).f()

class ChildD2(ParentD):
    def f(self):
        if False: super # Python injects __class__ into scope
        builtins.super(ChildD2, self).f()

class ChildD3(ParentD):
    def f(self):
        builtins.super(ChildD3, self).f()
        super # Python injects __class__ into scope

import builtins as builtins_alias
class ChildD4(ParentD):
    def f(self):
        builtins_alias.super(ChildD4, self).f()
        super # Python injects __class__ into scope

class ChildD5(ParentD):
    def f(self):
        super = 1
        super # Python injects __class__ into scope
        builtins.super(ChildD5, self).f()

class ChildD6(ParentD):
    def f(self):
        super: "Any"
        __class__ # Python injects __class__ into scope
        builtins.super(ChildD6, self).f()

class ChildD7(ParentD):
    def f(self):
        def x():
            __class__ # Python injects __class__ into scope
        builtins.super(ChildD7, self).f()

class ChildD8(ParentD):
    def f(self):
        def x():
            super = 1
        super # Python injects __class__ into scope
        builtins.super(ChildD8, self).f()

class ChildD9(ParentD):
    def f(self):
        def x():
            __class__ = 1
        __class__ # Python injects __class__ into scope
        builtins.super(ChildD9, self).f()

class ChildD10(ParentD):
    def f(self):
        def x():
            __class__ = 1
        super # Python injects __class__ into scope
        builtins.super(ChildD10, self).f()


# Must be ignored
class ParentI:
    def f(self):
        print("I")

class ChildI1(ParentI):
    def f(self):
        builtins.super(ChildI1, self).f() # no __class__ in the local scope


class ChildI2(ParentI):
    def b(self):
        x = __class__
        if False: super

    def f(self):
        self.b()
        builtins.super(ChildI2, self).f() # no __class__ in the local scope

class ChildI3(ParentI):
    def f(self):
        if False: super
        def x(_):
            builtins.super(ChildI3, self).f() # no __class__ in the local scope
        x(None)

class ChildI4(ParentI):
    def f(self):
        super: "str"
        builtins.super(ChildI4, self).f() # no __class__ in the local scope

class ChildI5(ParentI):
    def f(self):
        super = 1
        __class__ = 3
        builtins.super(ChildI5, self).f() # no __class__ in the local scope

class ChildI6(ParentI):
    def f(self):
        __class__ = None
        __class__
        builtins.super(ChildI6, self).f() # no __class__ in the local scope

class ChildI7(ParentI):
    def f(self):
        __class__ = None
        super
        builtins.super(ChildI7, self).f()

class ChildI8(ParentI):
    def f(self):
        __class__: "Any"
        super
        builtins.super(ChildI8, self).f()

class ChildI9(ParentI):
    def f(self):
        class A:
            def foo(self):
                if False: super
                if False: __class__
        builtins.super(ChildI9, self).f()


# See: https://github.com/astral-sh/ruff/issues/20422
# UP008 should not apply when the class variable is shadowed
class A:
    def f(self):
        return 1

class B(A):
    def f(self):
        return 2

class C(B):
    def f(self):
        C = B  # Local variable C shadows the class name
        return super(C, self).f()  # Should NOT trigger UP008


# See: https://github.com/astral-sh/ruff/issues/20491
# UP008 should not apply when __class__ is a local variable
class A:
    def f(self):
        return 1

class B(A):
    def f(self):
        return 2

class C(B):
    def f(self):
        __class__ = B  # Local variable __class__ shadows the implicit __class__
        return super(__class__, self).f()  # Should NOT trigger UP008

class Base:
    def __init__(self, foo):
        self.foo = foo


class Outer(Base):
    def __init__(self, foo):
        super().__init__(foo)  # Should not trigger UP008

    class Inner(Base):
        def __init__(self, foo):
            super(Outer.Inner, self).__init__(foo)  # UP008: matches enclosing class chain


# See: https://github.com/astral-sh/ruff/issues/24001
# Bare and partially qualified class names should not match nested classes.
class CommonNesting:
    class C(Base):
        def __init__(self, foo):
            super(C, self).__init__(foo)  # Should NOT trigger UP008


class HigherLevelsOfNesting:
    class Inner:
        class C(Base):
            def __init__(self, foo):
                super(Inner.C, self).__init__(foo)  # Should NOT trigger UP008


# super() first arg is an attribute that only matches on the last segment,
# but refers to a different class (Outer.Inner, not __class__ which is Inner).
class Inner(Outer.Inner):
    def __init__(self, foo):
        super(Outer.Inner, self).__init__(foo)  # Should NOT trigger UP008

# super() first arg has an unrelated module prefix
class Outer2:
    class Inner2(Base):
        def __init__(self, foo):
            super(some_module.Outer2.Inner2, self).__init__(foo)  # Should NOT trigger UP008

# 3-level deep nesting: super(A.B.C, self) should trigger UP008
class A:
    class B:
        class C(Base):
            def __init__(self, foo):
                super(A.B.C, self).__init__(foo)  # UP008: matches full chain

# Mismatched middle segment: Wrong.Inner doesn't match Outer3.Inner
class Outer3:
    class Inner(Base):
        def __init__(self, foo):
            super(Wrong.Inner, self).__init__(foo)  # Should NOT trigger UP008


class Whitespace(BaseClass):
    def f(self):
        super (Whitespace, self).f()  # can use super()


def function_local():
    class LocalOuter:
        class LocalInner(BaseClass):
            def f(self):
                super(LocalOuter.LocalInner, self).f()  # can use super()


class LambdaMethod(BaseClass):
    # TODO(charlie): class-body lambda rewrite is still missed.
    f = lambda self: super(LambdaMethod, self).f()  # can use super()


class ClassMethod(BaseClass):
    @classmethod
    def f(cls):
        super(ClassMethod, cls).f()  # can use super()


class AsyncMethod(BaseClass):
    async def f(self):
        super(AsyncMethod, self).f()  # can use super()


class OuterWithWhitespace:
    class Inner(BaseClass):
        def f(self):
            super (OuterWithWhitespace.Inner, self).f()  # can use super()
