# Properties

`property` is a built-in class in Python that can be used to model class attributes with custom
getters, setters, and deleters.

## Basic getter

`property` is typically used as a decorator on a getter method. It turns the method into a property
object. When accessing the property on an instance, the descriptor protocol is invoked, which calls
the getter method:

```py
class C:
    @property
    def my_property(self) -> int:
        return 1

reveal_type(C().my_property)  # revealed: int
```

When a property is accessed on the class directly, the descriptor protocol is also invoked, but
`property.__get__` simply returns itself in this case (when `instance` is `None`):

```py
reveal_type(C.my_property)  # revealed: property
```

## Getter and setter

A property can also have a setter method, which is used to set the value of the property. The setter
method is defined using the `@<property_name>.setter` decorator. The setter method takes the value
to be set as an argument.

```py
class C:
    @property
    def my_property(self) -> int:
        return 1

    @my_property.setter
    def my_property(self, value: int) -> None:
        pass

c = C()
reveal_type(c.my_property)  # revealed: int
c.my_property = 2

# error: [invalid-assignment]
c.my_property = "a"
```

## Properties returning `Self`

A property that returns `Self` refers to an instance of the class:

```py
from typing_extensions import Self

class Path:
    @property
    def parent(self) -> Self:
        raise NotImplementedError

reveal_type(Path().parent)  # revealed: Path
```

This also works when a setter is defined:

```py
class Node:
    @property
    def parent(self) -> Self:
        raise NotImplementedError

    @parent.setter
    def parent(self, value: Self) -> None:
        pass

root = Node()
child = Node()
child.parent = root

reveal_type(child.parent)  # revealed: Node
```

## `property.getter`

`property.getter` can be used to overwrite the getter method of a property. This does not overwrite
the existing setter:

```py
class C:
    @property
    def my_property(self) -> int:
        return 1

    @my_property.setter
    def my_property(self, value: int) -> None:
        pass

    @my_property.getter
    def my_property(self) -> str:
        return "a"

c = C()
reveal_type(c.my_property)  # revealed: str
c.my_property = 2

# error: [invalid-assignment]
c.my_property = "b"
```

## `property.deleter`

We do not support `property.deleter` yet, but we make sure that it does not invalidate the getter or
setter:

```py
class C:
    @property
    def my_property(self) -> int:
        return 1

    @my_property.setter
    def my_property(self, value: int) -> None:
        pass

    @my_property.deleter
    def my_property(self) -> None:
        pass

c = C()
reveal_type(c.my_property)  # revealed: int
c.my_property = 2
# error: [invalid-assignment]
c.my_property = "a"
```

## Conditional redefinition in class body

Distinct property definitions in statically unknown class-body branches should remain distinct, the
same way methods do:

```py
from random import random

class Baz:
    if random():
        def method(self) -> int:
            return 42

        @property
        def prop(self) -> int:
            return 42

    else:
        def method(self) -> str:
            return "hello"

        @property
        def prop(self) -> str:
            return "hello"

baz = Baz()
reveal_type(baz.prop)  # revealed: int | str
reveal_type(baz.method())  # revealed: int | str
```

## Failure cases

### Attempting to write to a read-only property

When attempting to write to a read-only property, we emit an error:

```py
class C:
    @property
    def attr(self) -> int:
        return 1

c = C()

# error: [invalid-assignment]
c.attr = 2
```

### Attempting to read a write-only property

When attempting to read a write-only property, we emit an error:

```py
class C:
    def attr_setter(self, value: int) -> None:
        pass
    attr = property(fset=attr_setter)

c = C()
c.attr = 1

# TODO: An error should be emitted here.
# See https://github.com/astral-sh/ruff/issues/16298 for more details.
reveal_type(c.attr)  # revealed: Unknown
```

### Wrong setter signature

```py
class C:
    @property
    def attr(self) -> int:
        return 1
    # error: [invalid-argument-type] "Argument to bound method `setter` is incorrect: Expected `(Any, Any, /) -> None`, found `def attr(self) -> None`"
    @attr.setter
    def attr(self) -> None:
        pass
```

### Wrong getter signature

```py
class C:
    # error: [invalid-argument-type] "Argument to class `property` is incorrect: Expected `((Any, /) -> Any) | None`, found `def attr(self, x: int) -> int`"
    @property
    def attr(self, x: int) -> int:
        return 1
```

## Limitations

### Manually constructed property

Properties can also be constructed manually using the `property` class. We partially support this:

```py
class C:
    def attr_getter(self) -> int:
        return 1
    attr = property(attr_getter)

c = C()
reveal_type(c.attr)  # revealed: Unknown | int
```

But note that we return `Unknown | int` because we did not declare the `attr` attribute. This is
consistent with how we usually treat attributes, but here, if we try to declare `attr` as
`property`, we fail to understand the property, since the `property` declaration shadows the more
precise type that we infer for `property(attr_getter)` (which includes the actual information about
the getter).

```py
class C:
    def attr_getter(self) -> int:
        return 1
    attr: property = property(attr_getter)

c = C()
reveal_type(c.attr)  # revealed: Unknown
```

### Attempting to write to a read-only manually constructed property

We should emit an error when trying to set an attribute that was created using a manually
constructed property with `fset=None`, just like we do for decorator-based read-only properties:

```py
class Foo:
    myprop = property(fget=lambda self: 42, fset=None)

class Bar:
    @property
    def myprop(self) -> int:
        return 42

f = Foo()
# error: [invalid-assignment]
f.myprop = 56

b = Bar()
# error: [invalid-assignment]
b.myprop = 42
```

## Behind the scenes

In this section, we trace through some of the steps that make properties work. We start with a
simple class `C` and a property `attr`:

```py
class C:
    def __init__(self):
        self._attr: int = 0

    @property
    def attr(self) -> int:
        return self._attr

    @attr.setter
    def attr(self, value: str) -> None:
        self._attr = len(value)
```

Next, we create an instance of `C`. As we have seen above, accessing `attr` on the instance will
return an `int`:

```py
c = C()

reveal_type(c.attr)  # revealed: int
```

Behind the scenes, when we write `c.attr`, the first thing that happens is that we statically look
up the symbol `attr` on the meta-type of `c`, i.e. the class `C`. We can emulate this static lookup
using `inspect.getattr_static`, to see that `attr` is actually an instance of the `property` class:

```py
from inspect import getattr_static

attr_property = getattr_static(C, "attr")
reveal_type(attr_property)  # revealed: property
```

The `property` class has a `__get__` method, which makes it a descriptor. It also has a `__set__`
method, which means that it is a *data* descriptor (if there is no setter, `__set__` is still
available but yields an `AttributeError` at runtime).

```py
reveal_type(type(attr_property).__get__)  # revealed: <wrapper-descriptor '__get__' of 'property' objects>
reveal_type(type(attr_property).__set__)  # revealed: <wrapper-descriptor '__set__' of 'property' objects>
```

When we access `c.attr`, the `__get__` method of the `property` class is called, passing the
property object itself as the first argument, and the class instance `c` as the second argument. The
third argument is the "owner" which can be set to `None` or to `C` in this case:

```py
reveal_type(type(attr_property).__get__(attr_property, c, C))  # revealed: int
reveal_type(type(attr_property).__get__(attr_property, c, None))  # revealed: int
```

Alternatively, the above can also be written as a method call:

```py
reveal_type(attr_property.__get__(c, C))  # revealed: int
```

When we access `attr` on the class itself, the descriptor protocol is also invoked, but the instance
argument is set to `None`. When `instance` is `None`, the call to `property.__get__` returns the
property instance itself. So the following expressions are all equivalent

```py
reveal_type(attr_property)  # revealed: property
reveal_type(C.attr)  # revealed: property
reveal_type(attr_property.__get__(None, C))  # revealed: property
reveal_type(type(attr_property).__get__(attr_property, None, C))  # revealed: property
```

When we set the property using `c.attr = "a"`, the `__set__` method of the property class is called.
This attribute access desugars to

```py
type(attr_property).__set__(attr_property, c, "a")

# error: [call-non-callable] "Call of wrapper descriptor `property.__set__` failed: calling the setter failed"
type(attr_property).__set__(attr_property, c, 1)
```

which is also equivalent to the following expressions:

```py
attr_property.__set__(c, "a")
# error: [call-non-callable]
attr_property.__set__(c, 1)

C.attr.__set__(c, "a")
# error: [call-non-callable]
C.attr.__set__(c, 1)
```

Properties also have `fget` and `fset` attributes that can be used to retrieve the original getter
and setter functions, respectively.

```py
reveal_type(attr_property.fget)  # revealed: def attr(self) -> int
reveal_type(attr_property.fget(c))  # revealed: int

reveal_type(attr_property.fset)  # revealed: def attr(self, value: str) -> None
reveal_type(attr_property.fset(c, "a"))  # revealed: None

# error: [invalid-argument-type]
attr_property.fset(c, 1)
```

At runtime, `attr_property.__get__` and `attr_property.__set__` are both instances of
`types.MethodWrapperType`:

```py
import types
from ty_extensions import TypeOf, static_assert, is_subtype_of

static_assert(is_subtype_of(TypeOf[attr_property.__get__], types.MethodWrapperType))
static_assert(is_subtype_of(TypeOf[attr_property.__set__], types.MethodWrapperType))
static_assert(not is_subtype_of(TypeOf[attr_property.__get__], types.WrapperDescriptorType))
static_assert(not is_subtype_of(TypeOf[attr_property.__set__], types.WrapperDescriptorType))
static_assert(not is_subtype_of(TypeOf[attr_property.__get__], types.BuiltinMethodType))
static_assert(not is_subtype_of(TypeOf[attr_property.__set__], types.BuiltinMethodType))
```

## Property type relations

Property equivalence and disjointness are structural over the getter and setter types. We use
standalone property objects here so `TypeOf[...]` sees the raw property type rather than the
`Unknown | ...` that can arise from class-attribute lookup. For the subtype cases, we construct
properties through helper functions with `Callable`-typed parameters so the slot types are
structural rather than exact function literals:

```py
from typing import Callable
from ty_extensions import (
    CallableTypeOf,
    TypeOf,
    is_assignable_to,
    is_disjoint_from,
    is_equivalent_to,
    is_subtype_of,
    static_assert,
)

def get_int(self) -> int:
    return 1

def get_str(self) -> str:
    return "a"

def set_int(self, value: int) -> None:
    pass

def set_object(self, value: object) -> None:
    pass

def set_str(self, value: str) -> None:
    pass

def get_equiv_a(self, /) -> int:
    return 1

def get_equiv_b(other, /) -> int:
    return 1

GetterReturnsInt = Callable[[object], int]
GetterReturnsObject = Callable[[object], object]
SetterAcceptsInt = Callable[[object, int], None]
SetterAcceptsObject = Callable[[object, object], None]

# Use `CallableTypeOf[...]` here rather than plain `Callable[...]` so these getters remain
# equivalent as types while still carrying distinct callable metadata and distinct Salsa IDs.
def assert_equivalent_properties(
    getter_a: CallableTypeOf[get_equiv_a],
    getter_b: CallableTypeOf[get_equiv_b],
):
    getter_only_equivalent_a = property(getter_a)
    getter_only_equivalent_b = property(getter_b)

    static_assert(is_equivalent_to(TypeOf[getter_only_equivalent_a], TypeOf[getter_only_equivalent_b]))
    static_assert(not is_disjoint_from(TypeOf[getter_only_equivalent_a], TypeOf[getter_only_equivalent_b]))

def assert_structural_property_relations(
    getter_sub: GetterReturnsInt,
    getter_super: GetterReturnsObject,
    setter_sub: SetterAcceptsObject,
    setter_super: SetterAcceptsInt,
):
    getter_covariant_sub = property(getter_sub)
    getter_covariant_super = property(getter_super)

    setter_contravariant_sub = property(fset=setter_sub)
    setter_contravariant_super = property(fset=setter_super)

    both_structural_sub = property(getter_sub, setter_sub)
    both_structural_super = property(getter_super, setter_super)

    static_assert(not is_equivalent_to(TypeOf[getter_covariant_sub], TypeOf[getter_covariant_super]))
    static_assert(not is_equivalent_to(TypeOf[setter_contravariant_sub], TypeOf[setter_contravariant_super]))
    static_assert(not is_equivalent_to(TypeOf[both_structural_sub], TypeOf[both_structural_super]))

    static_assert(is_subtype_of(TypeOf[getter_covariant_sub], TypeOf[getter_covariant_super]))
    static_assert(not is_subtype_of(TypeOf[getter_covariant_super], TypeOf[getter_covariant_sub]))
    static_assert(is_assignable_to(TypeOf[getter_covariant_sub], TypeOf[getter_covariant_super]))
    static_assert(not is_assignable_to(TypeOf[getter_covariant_super], TypeOf[getter_covariant_sub]))

    static_assert(is_subtype_of(TypeOf[setter_contravariant_sub], TypeOf[setter_contravariant_super]))
    static_assert(not is_subtype_of(TypeOf[setter_contravariant_super], TypeOf[setter_contravariant_sub]))
    static_assert(is_assignable_to(TypeOf[setter_contravariant_sub], TypeOf[setter_contravariant_super]))
    static_assert(not is_assignable_to(TypeOf[setter_contravariant_super], TypeOf[setter_contravariant_sub]))

    static_assert(is_subtype_of(TypeOf[both_structural_sub], TypeOf[both_structural_super]))
    static_assert(not is_subtype_of(TypeOf[both_structural_super], TypeOf[both_structural_sub]))
    static_assert(is_assignable_to(TypeOf[both_structural_sub], TypeOf[both_structural_super]))
    static_assert(not is_assignable_to(TypeOf[both_structural_super], TypeOf[both_structural_sub]))

    static_assert(is_subtype_of(TypeOf[both_structural_sub.__get__], TypeOf[both_structural_super.__get__]))
    static_assert(not is_subtype_of(TypeOf[both_structural_super.__get__], TypeOf[both_structural_sub.__get__]))
    static_assert(is_subtype_of(TypeOf[both_structural_sub.__set__], TypeOf[both_structural_super.__set__]))
    static_assert(not is_subtype_of(TypeOf[both_structural_super.__set__], TypeOf[both_structural_sub.__set__]))

    static_assert(not is_disjoint_from(TypeOf[getter_covariant_sub], TypeOf[getter_covariant_super]))
    static_assert(not is_disjoint_from(TypeOf[setter_contravariant_sub], TypeOf[setter_contravariant_super]))
    static_assert(not is_disjoint_from(TypeOf[both_structural_sub], TypeOf[both_structural_super]))
    static_assert(not is_disjoint_from(TypeOf[both_structural_sub.__get__], TypeOf[both_structural_super.__get__]))
    static_assert(not is_disjoint_from(TypeOf[both_structural_sub.__set__], TypeOf[both_structural_super.__set__]))

empty_a = property()
empty_b = property()

getter_only_a = property(get_int)
getter_only_b = property(get_int)
getter_only_c = property(get_str)
getter_only_d = property(get_int, None)

setter_only_a = property(fset=set_int)
setter_only_b = property(fset=set_int)
setter_only_c = property(fset=set_str)
setter_only_d = property(None, set_int)

both_a = property(get_int, set_int)
both_b = property(get_int, set_int)
both_c = property(get_int, set_str)
both_d = property(get_str, set_int)

static_assert(is_equivalent_to(TypeOf[empty_a], TypeOf[empty_b]))
static_assert(is_equivalent_to(TypeOf[getter_only_a], TypeOf[getter_only_b]))
static_assert(is_equivalent_to(TypeOf[getter_only_a], TypeOf[getter_only_d]))
static_assert(is_equivalent_to(TypeOf[setter_only_a], TypeOf[setter_only_b]))
static_assert(is_equivalent_to(TypeOf[setter_only_a], TypeOf[setter_only_d]))
static_assert(is_equivalent_to(TypeOf[both_a], TypeOf[both_b]))

static_assert(not is_equivalent_to(TypeOf[empty_a], TypeOf[getter_only_a]))
static_assert(not is_equivalent_to(TypeOf[empty_a], TypeOf[setter_only_a]))
static_assert(not is_equivalent_to(TypeOf[getter_only_a], TypeOf[getter_only_c]))
static_assert(not is_equivalent_to(TypeOf[getter_only_a], TypeOf[setter_only_a]))
static_assert(not is_equivalent_to(TypeOf[getter_only_a], TypeOf[both_a]))
static_assert(not is_equivalent_to(TypeOf[setter_only_a], TypeOf[setter_only_c]))
static_assert(not is_equivalent_to(TypeOf[setter_only_a], TypeOf[both_a]))
static_assert(not is_equivalent_to(TypeOf[both_a], TypeOf[both_c]))
static_assert(not is_equivalent_to(TypeOf[both_a], TypeOf[both_d]))

static_assert(not is_disjoint_from(TypeOf[empty_a], TypeOf[empty_b]))
static_assert(not is_disjoint_from(TypeOf[getter_only_a], TypeOf[getter_only_b]))
static_assert(not is_disjoint_from(TypeOf[getter_only_a], TypeOf[getter_only_d]))
static_assert(not is_disjoint_from(TypeOf[setter_only_a], TypeOf[setter_only_b]))
static_assert(not is_disjoint_from(TypeOf[setter_only_a], TypeOf[setter_only_d]))
static_assert(not is_disjoint_from(TypeOf[both_a], TypeOf[both_b]))

static_assert(is_disjoint_from(TypeOf[empty_a], TypeOf[getter_only_a]))
static_assert(is_disjoint_from(TypeOf[empty_a], TypeOf[setter_only_a]))
static_assert(is_disjoint_from(TypeOf[getter_only_a], TypeOf[getter_only_c]))
static_assert(is_disjoint_from(TypeOf[getter_only_a], TypeOf[setter_only_a]))
static_assert(is_disjoint_from(TypeOf[getter_only_a], TypeOf[both_a]))
static_assert(is_disjoint_from(TypeOf[setter_only_a], TypeOf[setter_only_c]))
static_assert(is_disjoint_from(TypeOf[setter_only_a], TypeOf[both_a]))
static_assert(is_disjoint_from(TypeOf[both_a], TypeOf[both_c]))
static_assert(is_disjoint_from(TypeOf[both_a], TypeOf[both_d]))

assert_equivalent_properties(get_equiv_a, get_equiv_b)
assert_structural_property_relations(get_int, get_int, set_object, set_object)
```
