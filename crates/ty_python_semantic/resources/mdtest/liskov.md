# The Liskov Substitution Principle

The Liskov Substitution Principle provides the basis for many of the assumptions a type checker
generally makes about types in Python:

> Subtype Requirement: Let ⁠`ϕ(x)`⁠ be a property provable about objects ⁠`x`⁠ of type `T`. Then
> ⁠`ϕ(y)`⁠ should be true for objects ⁠`y` of type `S` where `S` is a subtype of `T`.

In order for a type checker's assumptions to be sound, it is crucial for the type checker to enforce
the Liskov Substitution Principle on code that it checks. In practice, this usually manifests as
several checks for a type checker to perform when it checks a subclass `B` of a class `A`:

1. Read-only attributes should only ever be overridden covariantly: if a property `A.p` resolves to
    `int` when accessed, accessing `B.p` should either resolve to `int` or a subtype of `int`.
1. Method return types should only ever be overridden covariantly: if a method `A.f` returns `int`
    when called, calling `B.f` should also resolve to `int or a subtype of`int\`.
1. Method parameters should only ever be overridden contravariantly: if a method `A.f` can be called
    with an argument of type `bool`, then the method `B.f` must also be callable with type `bool`
    (though it is permitted for the override to also accept other types)
1. Mutable attributes should only ever be overridden invariantly: if a mutable attribute `A.attr`
    resolves to type `str`, it can only be overridden on a subclass with exactly the same type.

## `ClassVar` and instance variables

A pure class variable cannot override an inherited instance variable, and an instance variable
cannot override an inherited pure class variable.

### Direct overrides

An annotation without `ClassVar` declares an instance variable, even if the declaration also has a
class-level default value. An explicit `ClassVar` declaration is a pure class variable. Overriding
one with the other changes the places where the attribute is valid, so it violates Liskov
substitution:

```py
from typing import ClassVar

class Base:
    instance_attr: int
    instance_attr_with_default: int = 1
    class_attr: ClassVar[int] = 1

class Subclass(Base):
    # error: [invalid-attribute-override] "class variable cannot override instance variable `Base.instance_attr`"
    instance_attr: ClassVar[int]

    # error: [invalid-attribute-override] "class variable cannot override instance variable `Base.instance_attr_with_default`"
    instance_attr_with_default: ClassVar[int] = 1

    # error: [invalid-attribute-override] "instance variable cannot override class variable `Base.class_attr`"
    class_attr: int

class ValidSubclass(Base):
    instance_attr: int
    instance_attr_with_default: int = 1
    class_attr: ClassVar[int] = 1
```

### Regular class-body assignments

An unannotated class-body assignment is an instance variable with a class-level default. This means
it can replace another inherited instance-variable default. If it overrides an inherited `ClassVar`,
it inherits that declaration and remains a class variable. However, an explicit `ClassVar` cannot
override an inherited unannotated class-body assignment, because code using the base class can still
write that attribute through an instance:

```py
from typing import ClassVar

class Base:
    instance_attr_with_default: int = 1
    class_attr: ClassVar[int] = 1

class RegularClassAttributeOverride(Base):
    class_attr = 1

class RegularClassAttributeBase:
    attr = 1

class ExplicitClassVarOverride(RegularClassAttributeBase):
    # error: [invalid-attribute-override] "class variable cannot override instance variable `RegularClassAttributeBase.attr`"
    attr: ClassVar[int] = 1

class ClassDefaultBase:
    class_default: int = 1
    declared_instance: bool

class ClassDefaultSubclass(ClassDefaultBase):
    class_default = 2
    declared_instance = True
```

### Repeated inherited conflicts

If a parent class already made an invalid change from class variable to instance variable, a child
that keeps the parent's kind should not receive a duplicate diagnostic. The same applies in the
other direction:

```py
from typing import ClassVar

class GrandparentClassVar:
    attr: ClassVar[int]

class ParentInstance(GrandparentClassVar):
    # error: [invalid-attribute-override] "instance variable cannot override class variable `GrandparentClassVar.attr`"
    attr: int

class ChildInstance(ParentInstance):
    attr: int

class GrandparentInstance:
    attr: int

class ParentClassVar(GrandparentInstance):
    # error: [invalid-attribute-override] "class variable cannot override instance variable `GrandparentInstance.attr`"
    attr: ClassVar[int]

class ChildClassVar(ParentClassVar):
    attr: ClassVar[int]
```

### Descriptors

A descriptor can define different behavior when accessed on an instance. Because descriptor lookup
is neither a pure class variable nor a normal instance variable, overriding it with an instance
attribute is accepted:

```py
class Descriptor:
    def __get__(self, instance: object, owner: type[object]) -> int:
        return 1

class DescriptorBase:
    descriptor_attr = Descriptor()

class DescriptorOverride(DescriptorBase):
    descriptor_attr: int
```

### Multiple inheritance

The subclass must satisfy every base class. It is not enough for the first base in the MRO to agree
with the subclass: an unrelated base that declares the same member as a pure class variable still
makes an instance-variable override invalid.

```py
from typing import ClassVar

class ClassVarBase:
    attr: ClassVar[int]

class InstanceBase:
    attr: int

class MultipleInheritanceSubclass(InstanceBase, ClassVarBase):
    # error: [invalid-attribute-override] "instance variable cannot override class variable `ClassVarBase.attr`"
    attr: int
```

### Dataclasses

Dataclass fields are instance variables, even though they are usually declared in the class body.
`ClassVar` fields remain pure class variables and are excluded from dataclass instance fields:

```py
from dataclasses import dataclass
from typing import ClassVar

@dataclass
class DC6:
    x: int
    y: ClassVar[int] = 1

@dataclass
class DC7(DC6):
    # error: [invalid-attribute-override] "class variable cannot override instance variable `DC6.x`"
    x: ClassVar[int]

    # error: [invalid-attribute-override] "instance variable cannot override class variable `DC6.y`"
    y: int
```

### Protocol implementations

Regular class-body assignments can implement protocol instance attributes. The `ClassVar` case below
uses the same rule as normal classes: an unannotated class-body assignment over an inherited
`ClassVar` provides a value while preserving the inherited declaration.

```py
from typing import ClassVar, Protocol

class ProtocolBase(Protocol):
    class_attr: ClassVar[int]
    instance_attr: int
    instance_attr_with_default: int = 1

class ProtocolImpl(ProtocolBase):
    class_attr = 1
    instance_attr = 1
    instance_attr_with_default = 1

class ProtocolWithClassVar(Protocol):
    x: int = 1
    y: int
    z: ClassVar[int]

class ProtocolWithClassVarImpl(ProtocolWithClassVar):
    y = 0
    z = 0
```

## Method return types

It is fine for a subclass method to return a subtype of the return type of the method it overrides:

```pyi
class Super:
    def method(self) -> int: ...

class Sub1(Super):
    def method(self) -> int: ...  # fine

class Sub2(Super):
    def method(self) -> bool: ...  # fine: `bool` is a subtype of `int`
```

However, returning a supertype leads to an error:

```pyi
class Sub3(Super):
    def method(self) -> object: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.pyi:10:9
   |
10 |     def method(self) -> object: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super.method`
   |
  ::: src/mdtest_snippet.pyi:2:9
   |
 2 |     def method(self) -> int: ...
   |         ------------------- `Super.method` defined here
   |
info: incompatible return types: `object` is not assignable to `int`
info: This violates the Liskov Substitution Principle
```

Returning a completely unrelated type also leads to an error:

```pyi
class Sub4(Super):
    def method(self) -> str: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.pyi:12:9
   |
12 |     def method(self) -> str: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super.method`
   |
  ::: src/mdtest_snippet.pyi:2:9
   |
 2 |     def method(self) -> int: ...
   |         ------------------- `Super.method` defined here
   |
info: incompatible return types: `str` is not assignable to `int`
info: This violates the Liskov Substitution Principle
```

## Method parameters

A subclass method may provide a different parameter list to the superclass method, but all
combinations of arguments accepted by the superclass method must continue to be accepted by the
overriding method.

```pyi
class Super:
    def method(self, x: int, /): ...

class Sub1(Super):
    def method(self, x: int, /): ...  # fine

class Sub2(Super):
    def method(self, x: object, /): ...  # fine: `method` still accepts any argument of type `int`

class Sub4(Super):
    def method(self, x: int | str, /): ...  # fine

class Sub5(Super):
    def method(self, x: int): ...  # fine: `x` can still be passed positionally

class Sub6(Super):
    # fine: `method()` can still be called with just a single argument
    def method(self, x: int, *args): ...

class Sub7(Super):
    def method(self, x: int, **kwargs): ...  # fine

class Sub8(Super):
    def method(self, x: int, *args, **kwargs): ...  # fine

class Sub9(Super):
    def method(self, x: int, extra_positional_arg=42, /): ...  # fine

class Sub10(Super):
    def method(self, x: int, extra_pos_or_kw_arg=42): ...  # fine

class Sub11(Super):
    def method(self, x: int, *, extra_kw_only_arg=42): ...  # fine
```

In the following cases, some calls permitted by the superclass are no longer allowed, so we emit an
error.

This method can no longer be passed arguments:

```pyi
class Sub12(Super):
    def method(self, /): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.pyi:35:9
   |
35 |     def method(self, /): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^ Definition is incompatible with `Super.method`
   |
  ::: src/mdtest_snippet.pyi:2:9
   |
 2 |     def method(self, x: int, /): ...
   |         ----------------------- `Super.method` defined here
   |
info: This violates the Liskov Substitution Principle
```

This method can no longer be passed exactly one argument:

```pyi
class Sub13(Super):
    def method(self, x, y, /): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.pyi:37:9
   |
37 |     def method(self, x, y, /): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super.method`
   |
  ::: src/mdtest_snippet.pyi:2:9
   |
 2 |     def method(self, x: int, /): ...
   |         ----------------------- `Super.method` defined here
   |
info: This violates the Liskov Substitution Principle
```

Here, `x` can no longer be passed positionally:

```pyi
class Sub14(Super):
    def method(self, /, *, x): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.pyi:39:9
   |
39 |     def method(self, /, *, x): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super.method`
   |
  ::: src/mdtest_snippet.pyi:2:9
   |
 2 |     def method(self, x: int, /): ...
   |         ----------------------- `Super.method` defined here
   |
info: parameter `x` is keyword-only but must also accept positional arguments
info: This violates the Liskov Substitution Principle
```

Here, `x` can no longer be passed any integer -- it now requires a `bool`!

```pyi
class Sub15(Super):
    def method(self, x: bool, /): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.pyi:41:9
   |
41 |     def method(self, x: bool, /): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super.method`
   |
  ::: src/mdtest_snippet.pyi:2:9
   |
 2 |     def method(self, x: int, /): ...
   |         ----------------------- `Super.method` defined here
   |
info: parameter `x` has an incompatible type: `int` is not assignable to `bool`
info: This violates the Liskov Substitution Principle
```

In this case, `x` can no longer be passed as a keyword argument:

```pyi
class Super2:
    def method2(self, x): ...

class Sub16(Super2):
    def method2(self, x, /): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method2`
  --> src/mdtest_snippet.pyi:43:9
   |
43 |     def method2(self, x): ...
   |         ---------------- `Super2.method2` defined here
44 |
45 | class Sub16(Super2):
46 |     def method2(self, x, /): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super2.method2`
   |
info: parameter `x` is positional-only but must also accept keyword arguments
info: This violates the Liskov Substitution Principle
```

In this case, `x` can no longer be passed as a positional argument:

```pyi
class Sub17(Super2):
    def method2(self, *, x): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method2`
  --> src/mdtest_snippet.pyi:43:9
   |
43 |     def method2(self, x): ...
   |         ---------------- `Super2.method2` defined here
44 |
45 | class Sub16(Super2):
46 |     def method2(self, x, /): ...  # snapshot: invalid-method-override
47 | class Sub17(Super2):
48 |     def method2(self, *, x): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super2.method2`
   |
info: parameter `x` is keyword-only but must also accept positional arguments
info: This violates the Liskov Substitution Principle
```

The reverse is fine:

```pyi
class Super3:
    def method3(self, *, x): ...

class Sub18(Super3):
    def method3(self, x): ...  # fine: `x` can still be used as a keyword argument
```

This is an error because `x` can no longer be passed as a keyword argument:

```pyi
class Sub19(Super3):
    def method3(self, x, /): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method3`
  --> src/mdtest_snippet.pyi:50:9
   |
50 |     def method3(self, *, x): ...
   |         ------------------- `Super3.method3` defined here
51 |
52 | class Sub18(Super3):
53 |     def method3(self, x): ...  # fine: `x` can still be used as a keyword argument
54 | class Sub19(Super3):
55 |     def method3(self, x, /): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super3.method3`
   |
info: This violates the Liskov Substitution Principle
```

Accepting a wider type for `*args` and `**kwargs` is fine:

```pyi
class Super4:
    def method(self, *args: int, **kwargs: str): ...

class Sub20(Super4):
    def method(self, *args: object, **kwargs: object): ...  # fine
```

Omitting `**kwargs` is an error:

```pyi
class Sub21(Super4):
    def method(self, *args): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.pyi:57:9
   |
57 |     def method(self, *args: int, **kwargs: str): ...
   |         --------------------------------------- `Super4.method` defined here
58 |
59 | class Sub20(Super4):
60 |     def method(self, *args: object, **kwargs: object): ...  # fine
61 | class Sub21(Super4):
62 |     def method(self, *args): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super4.method`
   |
info: This violates the Liskov Substitution Principle
```

Similarly, omitting `*args` is also an error:

```pyi
class Sub22(Super4):
    def method(self, **kwargs): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
  --> src/mdtest_snippet.pyi:64:9
   |
64 |     def method(self, **kwargs): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Super4.method`
   |
  ::: src/mdtest_snippet.pyi:57:9
   |
57 |     def method(self, *args: int, **kwargs: str): ...
   |         --------------------------------------- `Super4.method` defined here
   |
info: This violates the Liskov Substitution Principle
```

Finally, this is not a Liskov violation because this is a gradual callable. It contains both `*args`
and `**kwargs` without annotations, so it is compatible with any signature of `method` on the
superclass.

```pyi
class Sub23(Super4):
    def method(self, x, *args, y, **kwargs): ...
```

## The entire class hierarchy is checked

If a child class's method definition is Liskov-compatible with the method definition on its parent
class, Liskov compatibility must also nonetheless be checked with respect to the method definition
on its grandparent class. This is because type checkers will treat the child class as a subtype of
the grandparent class just as much as they treat it as a subtype of the parent class, so
substitutability with respect to the grandparent class is just as important.

However, if the parent class itself already has an LSP violation with an ancestor, we do not report
the same violation for the child class. This is because the child class cannot fix the violation
without introducing a new, worse violation against its immediate parent's contract.

`stub.pyi`:

```pyi
from typing import Any

class Grandparent:
    def method(self, x: int) -> None: ...

class Parent(Grandparent):
    def method(self, x: str) -> None: ...  # snapshot: invalid-method-override

class Child(Parent):
    # compatible with the signature of `Parent.method`, but not with `Grandparent.method`.
    # However, since `Parent.method` already violates LSP with `Grandparent.method`,
    # we don't report the same violation for `Child` -- it's inherited from `Parent`.
    def method(self, x: str) -> None: ...

class OtherChild(Parent):
    # compatible with the signature of `Grandparent.method`, but not with `Parent.method`:
    def method(self, x: int) -> None: ...  # snapshot: invalid-method-override

class ChildWithNewViolation(Parent):
    # incompatible with BOTH `Parent.method` (str) and `Grandparent.method` (int).
    # We report the violation against the immediate parent (`Parent`), not the grandparent.
    def method(self, x: bytes) -> None: ...  # snapshot: invalid-method-override

class GrandparentWithReturnType:
    def method(self) -> int: ...

class ParentWithReturnType(GrandparentWithReturnType):
    def method(self) -> str: ...  # snapshot: invalid-method-override

class ChildWithReturnType(ParentWithReturnType):
    # Returns `int` again -- compatible with `GrandparentWithReturnType.method`,
    # but not with `ParentWithReturnType.method`. We report against the immediate parent.
    def method(self) -> int: ...  # snapshot: invalid-method-override

class GradualParent(Grandparent):
    def method(self, x: Any) -> None: ...

class ThirdChild(GradualParent):
    # `GradualParent.method` is compatible with the signature of `Grandparent.method`,
    # and `ThirdChild.method` is compatible with the signature of `GradualParent.method`,
    # but `ThirdChild.method` is not compatible with the signature of `Grandparent.method`
    def method(self, x: str) -> None: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `method`
 --> src/stub.pyi:4:9
  |
4 |     def method(self, x: int) -> None: ...
  |         ---------------------------- `Grandparent.method` defined here
5 |
6 | class Parent(Grandparent):
7 |     def method(self, x: str) -> None: ...  # snapshot: invalid-method-override
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Grandparent.method`
  |
info: parameter `x` has an incompatible type: `int` is not assignable to `str`
info: This violates the Liskov Substitution Principle


error[invalid-method-override]: Invalid override of method `method`
  --> src/stub.pyi:17:9
   |
17 |     def method(self, x: int) -> None: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.method`
   |
  ::: src/stub.pyi:7:9
   |
 7 |     def method(self, x: str) -> None: ...  # snapshot: invalid-method-override
   |         ---------------------------- `Parent.method` defined here
   |
info: parameter `x` has an incompatible type: `str` is not assignable to `int`
info: This violates the Liskov Substitution Principle


error[invalid-method-override]: Invalid override of method `method`
  --> src/stub.pyi:22:9
   |
22 |     def method(self, x: bytes) -> None: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.method`
   |
  ::: src/stub.pyi:7:9
   |
 7 |     def method(self, x: str) -> None: ...  # snapshot: invalid-method-override
   |         ---------------------------- `Parent.method` defined here
   |
info: parameter `x` has an incompatible type: `str` is not assignable to `bytes`
info: This violates the Liskov Substitution Principle


error[invalid-method-override]: Invalid override of method `method`
  --> src/stub.pyi:25:9
   |
25 |     def method(self) -> int: ...
   |         ------------------- `GrandparentWithReturnType.method` defined here
26 |
27 | class ParentWithReturnType(GrandparentWithReturnType):
28 |     def method(self) -> str: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `GrandparentWithReturnType.method`
   |
info: incompatible return types: `str` is not assignable to `int`
info: This violates the Liskov Substitution Principle


error[invalid-method-override]: Invalid override of method `method`
  --> src/stub.pyi:28:9
   |
28 |     def method(self) -> str: ...  # snapshot: invalid-method-override
   |         ------------------- `ParentWithReturnType.method` defined here
29 |
30 | class ChildWithReturnType(ParentWithReturnType):
31 |     # Returns `int` again -- compatible with `GrandparentWithReturnType.method`,
32 |     # but not with `ParentWithReturnType.method`. We report against the immediate parent.
33 |     def method(self) -> int: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `ParentWithReturnType.method`
   |
info: incompatible return types: `int` is not assignable to `str`
info: This violates the Liskov Substitution Principle


error[invalid-method-override]: Invalid override of method `method`
  --> src/stub.pyi:42:9
   |
42 |     def method(self, x: str) -> None: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Grandparent.method`
   |
  ::: src/stub.pyi:4:9
   |
 4 |     def method(self, x: int) -> None: ...
   |         ---------------------------- `Grandparent.method` defined here
   |
info: parameter `x` has an incompatible type: `int` is not assignable to `str`
info: This violates the Liskov Substitution Principle
```

`other_stub.pyi`:

```pyi
class A:
    def get(self, default): ...

class B(A):
    def get(self, default, /): ...  # snapshot: invalid-method-override

get = 56

class C(B):
    # `get` appears in the symbol table of `C`,
    # but that doesn't confuse our diagnostic...
    foo = get

class D(C):
    # compatible with `C.get` and `B.get`, but not with `A.get`.
    # Since `B.get` already violates LSP with `A.get`, we don't report for `D`.
    def get(self, my_default): ...
```

```snapshot
error[invalid-method-override]: Invalid override of method `get`
 --> src/other_stub.pyi:2:9
  |
2 |     def get(self, default): ...
  |         ------------------ `A.get` defined here
3 |
4 | class B(A):
5 |     def get(self, default, /): ...  # snapshot: invalid-method-override
  |         ^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `A.get`
  |
info: parameter `default` is positional-only but must also accept keyword arguments
info: This violates the Liskov Substitution Principle
```

Unannotated overrides of overloaded dunder methods should remain accepted.

```pyi
class C(list[int]):
    def __getitem__(self, key): ...
```

## Non-generic methods on generic classes work as expected

```toml
[environment]
python-version = "3.12"
```

```pyi
class A[T]:
    def method(self, x: T) -> None: ...

class B[T](A[T]):
    def method(self, x: T) -> None: ...  # fine

class C(A[int]):
    def method(self, x: int) -> None: ...  # fine

class D[T](A[T]):
    def method(self, x: object) -> None: ...  # fine

class E(A[int]):
    def method(self, x: object) -> None: ...  # fine

class F[T](A[T]):
    # `str` is not necessarily a supertype of `T`!
    # error: [invalid-method-override]
    def method(self, x: str) -> None: ...

class G(A[int]):
    def method(self, x: bool) -> None: ...  # error: [invalid-method-override]
```

## Generic methods on non-generic classes work as expected

```toml
[environment]
python-version = "3.12"
```

```pyi
from typing import Never, Self

class A:
    def method[T](self, x: T) -> T: ...

class B(A):
    def method[T](self, x: T) -> T: ...  # fine

class C(A):
    def method(self, x: object) -> Never: ...  # fine

class D(A):
    # TODO: we should emit [invalid-method-override] here:
    # `A.method` accepts an argument of any type,
    # but `D.method` only accepts `int`s
    def method(self, x: int) -> int: ...

class A2:
    def method(self, x: int) -> int: ...

class B2(A2):
    # fine: although `B2.method()` will not always return an `int`,
    # an instance of `B2` can be substituted wherever an instance of `A2` is expected,
    # and it *will* always return an `int` if it is passed an `int`
    # (which is all that will be allowed if an instance of `A2` is expected)
    def method[T](self, x: T) -> T: ...

class C2(A2):
    def method[T: int](self, x: T) -> T: ...

class D2(A2):
    # The type variable is bound to a type disjoint from `int`,
    # so the method will not accept integers, and therefore this is an invalid override
    def method[T: str](self, x: T) -> T: ...  # error: [invalid-method-override]

class A3:
    def method(self) -> Self: ...

class B3(A3):
    def method(self) -> Self: ...  # fine

class C3(A3):
    # TODO: should this be allowed?
    # Mypy/pyright/pyrefly all allow it,
    # but conceptually it seems similar to `B4.method` below,
    # which mypy/pyrefly agree is a Liskov violation
    # (pyright disagrees as of 20/11/2025: https://github.com/microsoft/pyright/issues/11128)
    # when called on a subclass, `C3.method()` will not return an
    # instance of that subclass
    def method(self) -> C3: ...

class D3(A3):
    def method(self: Self) -> Self: ...  # fine

class E3(A3):
    def method(self: E3) -> Self: ...  # fine

class F3(A3):
    def method(self: A3) -> Self: ...  # fine

class G3(A3):
    def method(self: object) -> Self: ...  # fine

class H3(A3):
    # TODO: we should emit `invalid-method-override` here
    # (`A3.method()` can be called on any instance of `A3`,
    # but `H3.method()` can only be called on objects that are
    # instances of `str`)
    def method(self: str) -> Self: ...

class I3(A3):
    # TODO: we should emit `invalid-method-override` here
    # (`I3.method()` cannot be called with any inhabited type!)
    def method(self: Never) -> Self: ...

class A4:
    def method[T: int](self, x: T) -> T: ...

class B4(A4):
    # TODO: we should emit `invalid-method-override` here.
    # `A4.method` promises that if it is passed a `bool`, it will return a `bool`,
    # but this is not necessarily true for `B4.method`: if passed a `bool`,
    # it could return a non-`bool` `int`!
    def method(self, x: int) -> int: ...
```

## Generic methods on generic classes work as expected

```toml
[environment]
python-version = "3.12"
```

```pyi
from typing import Never

class A[T]:
    def method[S](self, x: T, y: S) -> S: ...

class B[T](A[T]):
    def method[S](self, x: T, y: S) -> S: ...  # fine

class C(A[int]):
    def method[S](self, x: int, y: S) -> S: ...  # fine

class D[T](A[T]):
    def method[S](self, x: object, y: S) -> S: ...  # fine

class E(A[int]):
    def method[S](self, x: object, y: S) -> S: ...  # fine

class F(A[int]):
    def method(self, x: object, y: object) -> Never: ...  # fine

class A2[T]:
    def method(self, x: T, y: int) -> int: ...

class B2[T](A2[T]):
    def method[S](self, x: T, y: S) -> S: ...  # fine
```

## Fully qualified names are used in diagnostics where appropriate

`one.pyi`:

```pyi
class A:
    def foo(self, x): ...
```

`two.pyi`:

```pyi
import one

class A(one.A):
    def foo(self, y): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `foo`
 --> src/two.pyi:4:9
  |
4 |     def foo(self, y): ...  # snapshot: invalid-method-override
  |         ^^^^^^^^^^^^ Definition is incompatible with `one.A.foo`
  |
 ::: src/one.pyi:2:9
  |
2 |     def foo(self, x): ...
  |         ------------ `one.A.foo` defined here
  |
info: the parameter named `y` does not match `x` (and can be used as a keyword parameter)
info: This violates the Liskov Substitution Principle
```

## Excluded methods

Certain special constructor methods are excluded from Liskov checks. None of the following classes
cause us to emit any errors, therefore:

```toml
# This is so that the dataclasses machinery will generate `__replace__` methods for us
# (the synthesized `__replace__` methods should not be reported as invalid overrides!)
[environment]
python-version = "3.13"
```

```pyi
from dataclasses import dataclass, InitVar
from typing_extensions import Self

class Grandparent: ...

class Parent(Grandparent):
    def __new__(cls, x: int) -> Self: ...
    def __init__(self, x: int) -> None: ...

class Child(Parent):
    def __new__(cls, x: str, y: str) -> Self: ...
    def __init__(self, x: str, y: str) -> Self: ...

@dataclass(init=False)
class DataSuper:
    x: InitVar[int]

    def __post_init__(self, x: int) -> None:
        self.x = x

@dataclass(init=False)
class DataSub(DataSuper):
    y: InitVar[str]

    def __post_init__(self, x: int, y: str) -> None:
        self.y = y
        super().__post_init__(x)
```

## Edge case: function defined in another module and then assigned in a class body

`foo.pyi`:

```pyi
def x(self, y: str): ...
```

`bar.pyi`:

```pyi
import foo

class A:
    def x(self, y: int): ...

class B(A):
    x = foo.x  # snapshot: invalid-method-override

class C:
    x = foo.x

class D(C):
    def x(self, y: int): ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `x`
 --> src/bar.pyi:4:9
  |
4 |     def x(self, y: int): ...
  |         --------------- `A.x` defined here
5 |
6 | class B(A):
7 |     x = foo.x  # snapshot: invalid-method-override
  |     ^^^^^^^^^ Definition is incompatible with `A.x`
  |
 ::: src/foo.pyi:1:5
  |
1 | def x(self, y: str): ...
  |     --------------- Signature of `B.x`
  |
info: parameter `y` has an incompatible type: `int` is not assignable to `str`
info: This violates the Liskov Substitution Principle


error[invalid-method-override]: Invalid override of method `x`
  --> src/bar.pyi:10:5
   |
10 |     x = foo.x
   |     --------- `C.x` defined here
11 |
12 | class D(C):
13 |     def x(self, y: int): ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^ Definition is incompatible with `C.x`
   |
  ::: src/foo.pyi:1:5
   |
 1 | def x(self, y: str): ...
   |     --------------- Signature of `C.x`
   |
info: parameter `y` has an incompatible type: `str` is not assignable to `int`
info: This violates the Liskov Substitution Principle
```

## Bad override of `__eq__`

```py
class Bad:
    x: int
    def __eq__(self, other: "Bad") -> bool:  # snapshot: invalid-method-override
        return self.x == other.x
```

```snapshot
error[invalid-method-override]: Invalid override of method `__eq__`
   --> src/mdtest_snippet.py:3:9
    |
  3 |     def __eq__(self, other: "Bad") -> bool:  # snapshot: invalid-method-override
    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `object.__eq__`
    |
   ::: stdlib/builtins.pyi:142:9
    |
142 |     def __eq__(self, value: object, /) -> bool: ...
    |         -------------------------------------- `object.__eq__` defined here
    |
info: parameter `value` has an incompatible type: `object` is not assignable to `Bad`
info: This violates the Liskov Substitution Principle
help: It is recommended for `__eq__` to work with arbitrary objects, for example:
help
help:     def __eq__(self, other: object) -> bool:
help:         if not isinstance(other, Bad):
help:             return False
help:         return <logic to compare two `Bad` instances>
help
```

## Class-private names do not override

```py
class X:
    def __get_value(self) -> int:
        return 0

class Y(X):
    def __get_value(self) -> str:
        return "s"
```

## Synthesized methods

`NamedTuple` classes and dataclasses both have methods generated at runtime that do not have
source-code definitions. There are several scenarios to consider here:

1. A synthesized method on a superclass is overridden by a "normal" (not synthesized) method on a
    subclass
1. A "normal" method on a superclass is overridden by a synthesized method on a subclass
1. A synthesized method on a superclass is overridden by a synthesized method on a subclass

```pyi
from dataclasses import dataclass
from typing import NamedTuple

@dataclass(order=True)
class Foo:
    x: int

class Bar(Foo):
    def __lt__(self, other: Bar) -> bool: ...  # snapshot: invalid-method-override

# TODO: specifying `order=True` on the subclass means that a `__lt__` method is
# generated that is incompatible with the generated `__lt__` method on the superclass.
# We could consider detecting this and emitting a diagnostic, though maybe it shouldn't
# be `invalid-method-override` since we'd emit it on the class definition rather than
# on any method definition. Note also that no other type checker complains about this
# as of 2025-11-21.
@dataclass(order=True)
class Bar2(Foo):
    y: str

# TODO: Although this class does not override any methods of `Foo`, the design of the
# `order=True` stdlib dataclasses feature itself arguably violates the Liskov Substitution
# Principle! Instances of `Bar3` cannot be substituted wherever an instance of `Foo` is
# expected, because the generated `__lt__` method on `Foo` raises an error unless the r.h.s.
# and `l.h.s.` have exactly the same `__class__` (it does not permit instances of `Foo` to
# be compared with instances of subclasses of `Foo`).
#
# Many users would probably like their type checkers to alert them to cases where instances
# of subclasses cannot be substituted for instances of superclasses, as this violates many
# assumptions a type checker will make and makes it likely that a type checker will fail to
# catch type errors elsewhere in the user's code. We could therefore consider treating all
# `order=True` dataclasses as implicitly `@final` in order to enforce soundness. However,
# this probably shouldn't be reported with the same error code as Liskov violations, since
# the error does not stem from any method signatures written by the user. The example is
# only included here for completeness.
#
# Note that no other type checker catches this error as of 2025-11-21.
class Bar3(Foo): ...

class Eggs:
    def __lt__(self, other: Eggs) -> bool: ...

# TODO: the generated `Ham.__lt__` method here incompatibly overrides `Eggs.__lt__`.
# We could consider emitting a diagnostic here. As of 2025-11-21, mypy reports a
# diagnostic here but pyright and pyrefly do not.
@dataclass(order=True)
class Ham(Eggs):
    x: int

class Baz(NamedTuple):
    x: int

class Spam(Baz):
    def _asdict(self) -> tuple[int, ...]: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `__lt__`
 --> src/mdtest_snippet.pyi:9:9
  |
9 |     def __lt__(self, other: Bar) -> bool: ...  # snapshot: invalid-method-override
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Foo.__lt__`
  |
info: parameter `other` has an incompatible type: `Foo` is not assignable to `Bar`
info: This violates the Liskov Substitution Principle
info: `Foo.__lt__` is a generated method created because `Foo` is a dataclass
 --> src/mdtest_snippet.pyi:5:7
  |
5 | class Foo:
  |       ^^^ Definition of `Foo`
  |


error[invalid-method-override]: Invalid override of method `_asdict`
  --> src/mdtest_snippet.pyi:54:9
   |
54 |     def _asdict(self) -> tuple[int, ...]: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Baz._asdict`
   |
info: incompatible return types: `tuple[int, ...]` is not assignable to `dict[str, Any]`
info: This violates the Liskov Substitution Principle
info: `Baz._asdict` is a generated method created because `Baz` inherits from `typing.NamedTuple`
  --> src/mdtest_snippet.pyi:50:7
   |
50 | class Baz(NamedTuple):
   |       ^^^^^^^^^^^^^^^ Definition of `Baz`
   |
```

## Staticmethods and classmethods

Methods decorated with `@staticmethod` or `@classmethod` are checked in much the same way as other
methods.

```pyi
class Parent:
    def instance_method(self, x: int) -> int: ...
    @classmethod
    def class_method(cls, x: int) -> int: ...
    @staticmethod
    def static_method(x: int) -> int: ...

class GoodChild1(Parent):
    @classmethod
    def class_method(cls, x: int) -> int: ...
    @staticmethod
    def static_method(x: int) -> int: ...

class GoodChild2(Parent):
    @classmethod
    def class_method(cls, x: object) -> bool: ...
    @staticmethod
    def static_method(x: object) -> bool: ...
```

When the types are incompatible, we report an error:

```pyi
class BadTypesA(Parent):
    @classmethod
    def class_method(cls, x: bool) -> object: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `class_method`
  --> src/mdtest_snippet.pyi:21:9
   |
21 |     def class_method(cls, x: bool) -> object: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.class_method`
   |
  ::: src/mdtest_snippet.pyi:4:9
   |
 4 |     def class_method(cls, x: int) -> int: ...
   |         -------------------------------- `Parent.class_method` defined here
   |
info: incompatible return types: `object` is not assignable to `int`
info: This violates the Liskov Substitution Principle
```

```pyi
class BadTypesB(Parent):
    @staticmethod
    def static_method(x: bool) -> object: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `static_method`
  --> src/mdtest_snippet.pyi:24:9
   |
24 |     def static_method(x: bool) -> object: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.static_method`
   |
  ::: src/mdtest_snippet.pyi:6:9
   |
 6 |     def static_method(x: int) -> int: ...
   |         ---------------------------- `Parent.static_method` defined here
   |
info: incompatible return types: `object` is not assignable to `int`
info: This violates the Liskov Substitution Principle
```

Overwriting an instance method with a staticmethod, or vice versa, is an error:

```pyi
class BadChild1A(Parent):
    @staticmethod
    def instance_method(self, x: int) -> int: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `instance_method`
  --> src/mdtest_snippet.pyi:27:9
   |
27 |     def instance_method(self, x: int) -> int: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.instance_method`
   |
  ::: src/mdtest_snippet.pyi:2:9
   |
 2 |     def instance_method(self, x: int) -> int: ...
   |         ------------------------------------ `Parent.instance_method` defined here
   |
info: `BadChild1A.instance_method` is a staticmethod but `Parent.instance_method` is an instance method
info: This violates the Liskov Substitution Principle
```

```pyi
class BadChild1B(Parent):
    def static_method(x: int) -> int: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `static_method`
  --> src/mdtest_snippet.pyi:29:9
   |
29 |     def static_method(x: int) -> int: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.static_method`
   |
  ::: src/mdtest_snippet.pyi:6:9
   |
 6 |     def static_method(x: int) -> int: ...
   |         ---------------------------- `Parent.static_method` defined here
   |
info: `BadChild1B.static_method` is an instance method but `Parent.static_method` is a staticmethod
info: This violates the Liskov Substitution Principle
```

Overwriting a classmethod with an instance method is also an error: Although the method has the same
signature as `Parent.class_method` when accessed on instances, it does not have the same signature
as `Parent.class_method` when accessed on the class object itself:

```pyi
class BadChild2A(Parent):
    # TODO: we should emit `invalid-method-override` here.
    def class_method(cls, x: int) -> int: ...
```

Conversely, overwriting an instance method with a classmethod is also an error: Although the method
has the same signature as `Parent.class_method` when accessed on instances, it does not have the
same signature as `Parent.class_method` when accessed on the class object itself.

Note that whereas `BadChild2A.class_method` is reported as a Liskov violation by mypy, pyright and
pyrefly, pyright is the only one of those three to report a Liskov violation on this method as of
2025-11-23.

```pyi
class BadChild2B(Parent):
    # TODO: we should emit `invalid-method-override` here.
    @classmethod
    def instance_method(self, x: int) -> int: ...
```

Overwriting a classmethod with a staticmethod, or vice versa, is also an error:

```pyi
class BadChild3A(Parent):
    @staticmethod
    def class_method(cls, x: int) -> int: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `class_method`
  --> src/mdtest_snippet.pyi:39:9
   |
39 |     def class_method(cls, x: int) -> int: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.class_method`
   |
  ::: src/mdtest_snippet.pyi:4:9
   |
 4 |     def class_method(cls, x: int) -> int: ...
   |         -------------------------------- `Parent.class_method` defined here
   |
info: `BadChild3A.class_method` is a staticmethod but `Parent.class_method` is a classmethod
info: This violates the Liskov Substitution Principle
```

```pyi
class BadChild3B(Parent):
    @classmethod
    def static_method(x: int) -> int: ...  # snapshot: invalid-method-override
```

```snapshot
error[invalid-method-override]: Invalid override of method `static_method`
  --> src/mdtest_snippet.pyi:42:9
   |
42 |     def static_method(x: int) -> int: ...  # snapshot: invalid-method-override
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Definition is incompatible with `Parent.static_method`
   |
  ::: src/mdtest_snippet.pyi:6:9
   |
 6 |     def static_method(x: int) -> int: ...
   |         ---------------------------- `Parent.static_method` defined here
   |
info: `BadChild3B.static_method` is a classmethod but `Parent.static_method` is a staticmethod
info: This violates the Liskov Substitution Principle
```

## Overloaded methods with positional-only parameters with defaults

When a base class has an overloaded method where one overload accepts only keyword arguments
(`**kwargs`), and the subclass overrides it with a positional-only parameter that has a default, the
override should be valid because callers can still call it without positional arguments.

```pyi
from typing import overload

class Base:
    @overload
    def method(self, x: int, /) -> None: ...
    @overload
    def method(self, **kwargs: int) -> None: ...
    def method(self, *args, **kwargs) -> None: ...

class GoodChild(Base):
    # This should be fine: the positional-only parameter has a default,
    # so calls like `obj.method(a=1)` are still valid
    def method(self, x: int = 0, /, **kwargs: int) -> None: ...

class BadChild(Base):
    # `x` has no default, so `obj.method(a=1)` would fail
    def method(self, x: int, /, **kwargs: int) -> None: ...  # error: [invalid-method-override]
```

## Definitely bound members with no reachable definitions(!)

We don't emit a Liskov-violation diagnostic here, but if you're writing code like this, you probably
have bigger problems:

```py
from __future__ import annotations

class MaybeEqWhile:
    while ...:
        def __eq__(self, other: MaybeEqWhile) -> bool:
            return True
```
