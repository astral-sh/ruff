# Tests for the `@deprecated` decorator

## Introduction

<!-- snapshot-diagnostics -->

The decorator `@deprecated("some message")` can be applied to functions, methods, overloads, and
classes. Uses of these items should subsequently produce a warning.

```py
from typing_extensions import deprecated

@deprecated("use OtherClass")
def myfunc(x: int): ...

myfunc(1)  # error: [deprecated] "use OtherClass"
```

```py
from typing_extensions import deprecated

@deprecated("use BetterClass")
class MyClass: ...

MyClass()  # error: [deprecated] "use BetterClass"
```

```py
from typing_extensions import deprecated

class MyClass:
    @deprecated("use something else")
    def afunc(): ...
    @deprecated("don't use this!")
    def amethod(self): ...

MyClass.afunc()  # error: [deprecated] "use something else"
MyClass().amethod()  # error: [deprecated] "don't use this!"
```

## Function replacements

An outer `@deprecated` decorator applies to the function returned by inner decorators.

```py
from collections.abc import Callable
from typing import Any, TypeVar
from typing_extensions import deprecated

R = TypeVar("R")

def replacement() -> str:
    return "replacement"

def replace_with(value: R) -> Callable[[Callable[..., Any]], R]:
    def decorator(_function: Callable[..., Any]) -> R:
        return value

    return decorator

@deprecated("use replacement directly")
@replace_with(replacement)
def old() -> None:
    pass

old()  # error: [deprecated] "use replacement directly"
replacement()

@replace_with(replacement)
@deprecated("discarded by outer replacement")
def replaced_after_deprecation() -> None:
    pass

replaced_after_deprecation()

@deprecated("outer deprecation")
@replace_with(replacement)
@deprecated("inner deprecation")
def multiply_deprecated() -> None:
    pass

multiply_deprecated()  # error: [deprecated] "outer deprecation"

class StaticMethodReplacement:
    @staticmethod
    @deprecated("use replacement directly")
    @replace_with(replacement)
    def old() -> None:
        pass

StaticMethodReplacement.old()  # error: [deprecated] "use replacement directly"
```

## Callable-object replacements

`@deprecated` can also wrap other callable objects at runtime, but we currently only preserve the
deprecation when an inner decorator returns a function literal.

```py
from collections.abc import Callable
from typing import Any, TypeVar
from typing_extensions import deprecated

R = TypeVar("R")

def replace_with(value: R) -> Callable[[Callable[..., Any]], R]:
    def decorator(_function: Callable[..., Any]) -> R:
        return value

    return decorator

class Replacement:
    def __call__(self) -> str:
        return "replacement"

@deprecated("use Replacement directly")
@replace_with(Replacement())
def old() -> None:
    pass

old()  # TODO: error: [deprecated] "use Replacement directly"
```

## Syntax

<!-- snapshot-diagnostics -->

The typeshed declaration of the decorator is as follows:

```ignore
class deprecated:
    message: LiteralString
    category: type[Warning] | None
    stacklevel: int
    def __init__(self, message: LiteralString, /, *, category: type[Warning] | None = ..., stacklevel: int = 1) -> None: ...
    def __call__(self, arg: _T, /) -> _T: ...
```

Only the mandatory message string is of interest to static analysis, the other two affect only
runtime behavior.

```py
from typing_extensions import deprecated

@deprecated  # error: [invalid-argument-type] "LiteralString"
def invalid_deco(): ...

invalid_deco()  # error: [missing-argument]
```

```py
from typing_extensions import deprecated

@deprecated()  # error: [missing-argument] "message"
def invalid_deco(): ...

invalid_deco()
```

The argument is supposed to be a LiteralString, and we can handle simple constant propagations like
this:

```py
from typing_extensions import deprecated

x = "message"

@deprecated(x)
def invalid_deco(): ...

invalid_deco()  # error: [deprecated] "message"
```

However sufficiently opaque LiteralStrings we can't resolve, and so we lose the message:

```py
from typing_extensions import deprecated, LiteralString

def opaque() -> LiteralString:
    return "message"

@deprecated(opaque())
def valid_deco(): ...

valid_deco()  # error: [deprecated]
```

Fully dynamic strings are technically allowed at runtime, but typeshed mandates that the input is a
LiteralString, so we can/should emit a diagnostic for this:

```py
from typing_extensions import deprecated

def opaque() -> str:
    return "message"

@deprecated(opaque())  # error: [invalid-argument-type] "LiteralString"
def dubious_deco(): ...

dubious_deco()
```

Although we have no use for the other arguments, we should still error if they're wrong.

```py
from typing_extensions import deprecated

@deprecated("some message", dsfsdf="whatever")  # error: [unknown-argument] "dsfsdf"
def invalid_deco(): ...

invalid_deco()
```

And we should always handle correct ones fine.

```py
from typing_extensions import deprecated

@deprecated("some message", category=DeprecationWarning, stacklevel=1)
def valid_deco(): ...

valid_deco()  # error: [deprecated] "some message"
```

## Category

The category must be a `Warning` subclass or `None`.

```py
from typing_extensions import deprecated

@deprecated("some message", category=42)  # error: [invalid-argument-type] "type[Warning] | None"
def invalid_category(): ...
@deprecated("some message", category=None)
def no_category(): ...
```

## Different Versions

There are 2 different sources of `@deprecated`: `warnings` and `typing_extensions`. The version in
`warnings` was added in 3.13, the version in `typing_extensions` is a compatibility shim.

```toml
[environment]
python-version = "3.13"
```

`main.py`:

```py
import warnings
import typing_extensions

@warnings.deprecated("nope")
def func1(): ...
@typing_extensions.deprecated("nada")
def func2(): ...

func1()  # error: [deprecated] "nope"
func2()  # error: [deprecated] "nada"
```

## Imports

### Direct Import Deprecated

Importing a deprecated item should produce a warning. Subsequent uses of the deprecated item
shouldn't produce a warning.

`module.py`:

```py
from typing_extensions import deprecated

@deprecated("Use OtherType instead")
class DeprType: ...

@deprecated("Use other_func instead")
def depr_func(): ...
```

`main.py`:

```py
# error: [deprecated] "Use OtherType instead"
# error: [deprecated] "Use other_func instead"
from module import DeprType, depr_func

# TODO: these diagnostics ideally shouldn't fire since we warn on the import
DeprType()  # error: [deprecated] "Use OtherType instead"
depr_func()  # error: [deprecated] "Use other_func instead"

def higher_order(x): ...

# TODO: these diagnostics ideally shouldn't fire since we warn on the import
higher_order(DeprType)  # error: [deprecated] "Use OtherType instead"
higher_order(depr_func)  # error: [deprecated] "Use other_func instead"

# TODO: these diagnostics ideally shouldn't fire since we warn on the import
DeprType.__str__  # error: [deprecated] "Use OtherType instead"
depr_func.__str__  # error: [deprecated] "Use other_func instead"
```

### Non-Import Deprecated

If the items aren't imported and instead referenced using `module.item` then each use should produce
a warning.

`module.py`:

```py
from typing_extensions import deprecated

@deprecated("Use OtherType instead")
class DeprType: ...

@deprecated("Use other_func instead")
def depr_func(): ...
```

`main.py`:

```py
import module

module.DeprType()  # error: [deprecated] "Use OtherType instead"
module.depr_func()  # error: [deprecated] "Use other_func instead"

def higher_order(x): ...

higher_order(module.DeprType)  # error: [deprecated] "Use OtherType instead"
higher_order(module.depr_func)  # error: [deprecated] "Use other_func instead"

module.DeprType.__str__  # error: [deprecated] "Use OtherType instead"
module.depr_func.__str__  # error: [deprecated] "Use other_func instead"
```

### Star Import Deprecated

If the items are instead star-imported, then the actual uses should warn.

`module.py`:

```py
from typing_extensions import deprecated

@deprecated("Use OtherType instead")
class DeprType: ...

@deprecated("Use other_func instead")
def depr_func(): ...
```

`main.py`:

```py
from module import *

DeprType()  # error: [deprecated] "Use OtherType instead"
depr_func()  # error: [deprecated] "Use other_func instead"

def higher_order(x): ...

higher_order(DeprType)  # error: [deprecated] "Use OtherType instead"
higher_order(depr_func)  # error: [deprecated] "Use other_func instead"

DeprType.__str__  # error: [deprecated] "Use OtherType instead"
depr_func.__str__  # error: [deprecated] "Use other_func instead"
```

## Aliases

Ideally a deprecated warning shouldn't transitively follow assignments, as you already had to "name"
the deprecated symbol to assign it to something else. These kinds of diagnostics would therefore be
redundant and annoying.

```py
from typing_extensions import deprecated

@deprecated("Use OtherType instead")
class DeprType: ...

@deprecated("Use other_func instead")
def depr_func(): ...

alias_func = depr_func  # error: [deprecated] "Use other_func instead"
AliasClass = DeprType  # error: [deprecated] "Use OtherType instead"

# TODO: these diagnostics ideally shouldn't fire
alias_func()  # error: [deprecated] "Use other_func instead"
AliasClass()  # error: [deprecated] "Use OtherType instead"
```

## Dunders

### Binary operators

Using `+` invokes `__add__`, so it reports that method's deprecation.

```py
from typing_extensions import deprecated

class Number:
    @deprecated("old addition")
    def __add__(self, other: object) -> "Number":
        return self

number = Number()
number + 1  # error: [deprecated] "old addition"
```

Without an `__iadd__` method, `+=` falls back to `__add__` and reports the same deprecation.

```py
number += 1  # error: [deprecated] "old addition"
```

### Reflected operators

When the left operand accepts the operation, a deprecated `__radd__` on the right operand is not
called and does not produce a warning.

```py
from typing_extensions import deprecated

class Left:
    def __add__(self, other: object) -> int:
        return 0

class Right:
    @deprecated("reflected addition")
    def __radd__(self, other: object) -> int:
        return 0

Left() + Right()
```

Here, `int.__add__` does not accept a `Right` instance, so `+` calls the deprecated
`Right.__radd__`.

```py
1 + Right()  # error: [deprecated] "reflected addition"
```

A deprecated method whose parameter does not accept the other operand does not trigger a warning
when a compatible reflected method is available.

```py
class RestrictedLeft:
    @deprecated("integer addition")
    def __add__(self, other: int) -> int:
        return 0

class ActiveRight:
    def __radd__(self, other: object) -> int:
        return 0

RestrictedLeft() + ActiveRight()
```

### In-place operators

When `__iadd__` accepts the operand, `+=` uses it without calling a deprecated `__add__`.

```py
from typing_extensions import deprecated

class Number:
    @deprecated("binary addition")
    def __add__(self, other: int) -> "Number":
        return self

    def __iadd__(self, other: int) -> "Number":
        return self

number = Number()
number += 1
```

A deprecated `__iadd__` produces a warning at the augmented assignment.

```py
class OldNumber:
    @deprecated("in-place addition")
    def __iadd__(self, other: int) -> "OldNumber":
        return self

old = OldNumber()
old += 1  # error: [deprecated] "in-place addition"
```

### Callable instances

Calling an instance invokes its `__call__` method and reports that method's deprecation.

```py
from typing_extensions import deprecated

class Invocable:
    @deprecated("do not call")
    def __call__(self) -> int:
        return 0

invocable = Invocable()
invocable()  # error: [deprecated] "do not call"
```

An explicit reference to `__call__` is also deprecated. Calling that reference still produces only
one warning.

```py
invocable.__call__  # error: [deprecated] "do not call"
invocable.__call__()  # error: [deprecated] "do not call"
```

### Overloaded callable instances

For overloaded `__call__` methods, only calls that select a deprecated overload trigger a warning.

```py
from typing import overload
from typing_extensions import deprecated

class Overloaded:
    @overload
    @deprecated("integer call")
    def __call__(self, value: int) -> int: ...
    @overload
    def __call__(self, value: str) -> str: ...
    def __call__(self, value: int | str) -> int | str:
        return value

overloaded = Overloaded()
overloaded(1)  # error: [deprecated] "integer call"
overloaded("one")
```

### Unary operators

If a dunder like `__invert__` is deprecated, then the equivalent `~` operator should fire a
diagnostic.

#### Custom operator

```py
from typing_extensions import deprecated

class MyBits:
    @deprecated("MyBits `~` support is broken")
    def __invert__(self):
        return self

x = MyBits()
~x  # error: [deprecated] "MyBits `~` support is broken"
```

#### Possibly unbound operator

If the operand's type is a union and the dunder is missing on some members, it's possibly unbound.
This should still report the deprecation on the members where it is found and is deprecated,
alongside `unsupported-operator` diagnostic.

```py
from typing_extensions import deprecated

class MyBits:
    @deprecated("MyBits `~` support is broken")
    def __invert__(self):
        return self

class NoBits: ...

def f(x: MyBits | NoBits):
    # error: [unsupported-operator]
    # error: [deprecated]
    ~x
```

#### Unions and intersections

A unary operation on a union reports a deprecation if any member's operator is deprecated.

```py
from typing_extensions import deprecated

class Deprecated:
    @deprecated("old inversion")
    def __invert__(self) -> int:
        return 1

class Ordinary:
    def __invert__(self) -> int:
        return 3

def mixed_union(value: Deprecated | Ordinary) -> None:
    ~value  # error: [deprecated] "old inversion"
```

An intersection can use a non-deprecated implementation instead, so it does not warn when one is
available.

```py
def mixed_intersection(value: Deprecated) -> None:
    if isinstance(value, Ordinary):
        ~value
```

When every applicable implementation is deprecated, one warning includes both messages.

```py
class AlsoDeprecated:
    @deprecated("another old inversion")
    def __invert__(self) -> int:
        return 2

def deprecated_intersection(value: Deprecated) -> None:
    if isinstance(value, AlsoDeprecated):
        # error: [deprecated] "`Deprecated.__invert__`, `AlsoDeprecated.__invert__`"
        ~value
```

A gradually typed comparison can produce an intersection of `bool` and `Any`. The unknown
alternative might provide a non-deprecated operator, so inverting it should not warn.

```py
from typing import Any

def gradual_intersection(value: Any) -> None:
    if value is None:
        return

    mask = value == 0
    ~mask
```

#### Bool literals

`bool.__invert__` is one such case in typeshed. This applies both to `bool` literals and to
arbitrary values of type `bool`.

```py
~True  # error: [deprecated]

def f(x: bool):
    ~x  # error: [deprecated]
```

#### Constrained TypeVars

A unary operation on a constrained type variable can invoke the method from any of its constraints.
If several methods are deprecated, their messages appear in one diagnostic.

```py
from typing import TypeVar
from typing_extensions import deprecated

class First:
    @deprecated("first")
    def __invert__(self) -> int:
        return 42

class Second:
    @deprecated("second")
    def __invert__(self) -> int:
        return 42

T = TypeVar("T", First, Second)

def f(value: T) -> None:
    # error: [deprecated] "`First.__invert__`, `Second.__invert__`"
    ~value
```

Deprecation reporting for one constraint does not depend on whether another constraint supports the
operator or on the order of the constraints.

```py
class Third: ...

U = TypeVar("U", Third, First)
V = TypeVar("V", First, Third)

def g(value: U) -> None:
    # error: [unsupported-operator]
    # error: [deprecated]
    ~value

def h(value: V) -> None:
    # error: [unsupported-operator]
    # error: [deprecated]
    ~value
```

A constraint that is itself a union may contain a deprecated operator even when that operator is
missing from another union member.

```py
W = TypeVar("W", First | Third, Second)

def nested_union(value: W) -> None:
    # error: [unsupported-operator]
    # error: [deprecated] "`First.__invert__`, `Second.__invert__`"
    ~value
```

A deprecated operator should also be reported when its signature cannot accept the implicit unary
call.

```py
class Invalid:
    @deprecated("invalid inversion")
    def __invert__(self, required: int) -> int:
        return required

X = TypeVar("X", Invalid, Second)

def invalid_operator(value: X) -> None:
    # error: [unsupported-operator]
    # error: [deprecated] "`Invalid.__invert__`, `Second.__invert__`"
    ~value
```

## Property accessors

Reading a property invokes its getter. A deprecated getter produces a warning on a read, but not on
an assignment or deletion.

```py
from typing_extensions import deprecated

class OldGetter:
    @property
    @deprecated("old getter")
    def value(self) -> int:
        return 0

    @value.setter
    def value(self, value: int) -> None: ...
    @value.deleter
    def value(self) -> None: ...

old_getter = OldGetter()
old_getter.value  # error: [deprecated] "old getter"
old_getter.value = 1
del old_getter.value
```

Assignments and deletions invoke the setter and deleter, respectively. Neither accessor is called
when reading the property.

```py
class OldSetter:
    @property
    def value(self) -> int:
        return 0

    @value.setter
    @deprecated("old setter")
    def value(self, value: int) -> None: ...
    @value.deleter
    @deprecated("old deleter")
    def value(self) -> None: ...

old_setter = OldSetter()
old_setter.value
old_setter.value = 1  # error: [deprecated] "old setter"
del old_setter.value  # error: [deprecated] "old deleter"
```

Access through the class returns the property object without invoking its deprecated getter.

```py
OldGetter.value
```

## Augmented property assignments

Augmented assignment reads and then writes the property. When both the getter and setter are
deprecated, it reports each accessor's deprecation.

```py
from typing_extensions import deprecated

class OldBoth:
    @property
    @deprecated("both getter")
    def value(self) -> int:
        return 0

    @value.setter
    @deprecated("both setter")
    def value(self, value: int) -> None: ...

old_both = OldBoth()
# error: [deprecated] "both getter"
# error: [deprecated] "both setter"
old_both.value += 1
```

## Inherited properties

Reading or deleting the property on a subclass instance reports the inherited accessor's
deprecation.

```py
from typing_extensions import deprecated

class Parent:
    @property
    @deprecated("parent getter")
    def value(self) -> int:
        return 0

    @value.deleter
    @deprecated("parent deleter")
    def value(self) -> None: ...

class Child(Parent): ...

Child().value  # error: [deprecated] "parent getter"
del Child().value  # error: [deprecated] "parent deleter"
```

Overriding the getter with a non-deprecated method removes the deprecation on reads.

```py
class ActiveChild(Parent):
    @property
    def value(self) -> int:
        return 0

ActiveChild().value
```

## Properties accessed through `super()`

Reading a property through `super()` invokes the parent getter with the instance as its receiver.

```py
from typing_extensions import deprecated

class Parent:
    @property
    @deprecated("parent getter")
    def value(self) -> int:
        return 0

    @value.deleter
    @deprecated("parent deleter")
    def value(self) -> None: ...

class Child(Parent):
    def read(self) -> int:
        return super().value  # error: [deprecated] "parent getter"
```

Binding `super()` to a class instead returns the property object without invoking its getter.

```py
super(Child, Child).value
```

Deleting an attribute on a `super()` object does not invoke the parent's property deleter.

```py
del super(Child, Child()).value
```

The same holds when the receiver may be either a `super()` object or an ordinary instance.

```py
class Ordinary:
    value: int

def delete_union(flag: bool):
    target = super(Child, Child()) if flag else Ordinary()
    del target.value
```

## Metaclass properties

A class is an instance of its metaclass, so reading or writing a metaclass property invokes its
accessors.

```py
from typing_extensions import deprecated

class Meta(type):
    @property
    @deprecated("metaclass getter")
    def value(cls) -> int:
        return 0

    @value.setter
    @deprecated("metaclass setter")
    def value(cls, value: int) -> None: ...

class C(metaclass=Meta): ...

C.value  # error: [deprecated] "metaclass getter"
C.value = 1  # error: [deprecated] "metaclass setter"
```

Access through the metaclass itself returns the property object without invoking its getter.

```py
Meta.value
```

## Properties on unions

An attribute access through a union warns if any member uses a deprecated property.

```py
from typing_extensions import deprecated

class Old:
    @property
    @deprecated("union getter")
    def value(self) -> int:
        return 0

    @value.setter
    @deprecated("union setter")
    def value(self, value: int) -> None: ...

class Active:
    value: int

def check(value: Old | Active):
    value.value  # error: [deprecated] "union getter"
    value.value = 1  # error: [deprecated] "union setter"
```

An invalid assignment on one union member does not hide a deprecated setter on another member.

```py
class Wrong:
    value: str

def check_invalid(value: Wrong | Old):
    # error: [invalid-assignment]
    # error: [deprecated] "union setter"
    value.value = 1
```

An invalid assignment still reports deprecations after an `isinstance` check narrows the union's
members to intersections.

```py
class Marker: ...

def check_invalid_intersections(value: Wrong | Old):
    if isinstance(value, Marker):
        # error: [invalid-assignment]
        # error: [deprecated] "union setter"
        value.value = 1
```

## Properties on intersections

An intersection can use a non-deprecated member's attribute instead of a deprecated property. We do
not warn when that alternative is available.

```py
from typing_extensions import deprecated

class Old:
    @property
    @deprecated("old getter")
    def value(self) -> int:
        return 0

    @value.setter
    @deprecated("old setter")
    def value(self, value: int) -> None: ...
    @value.deleter
    @deprecated("old deleter")
    def value(self) -> None: ...

class Active:
    value: int

def check(value: Old):
    if isinstance(value, Active):
        value.value
        value.value = 1
```

A member without the attribute does not provide an alternative accessor, so the deprecated property
still applies.

```py
class Marker: ...

def check_marker(value: Old):
    if isinstance(value, Marker):
        value.value  # error: [deprecated] "old getter"
        value.value = 1  # error: [deprecated] "old setter"
```

When both members define their own deprecated property, reading, assigning, and deleting the
attribute report the getter, setter, and deleter deprecations, respectively. Each warning names both
declarations.

```py
class AlsoOld:
    @property
    @deprecated("old getter")
    def value(self) -> int:
        return 0

    @value.setter
    @deprecated("old setter")
    def value(self, value: int) -> None: ...
    @value.deleter
    @deprecated("old deleter")
    def value(self) -> None: ...

def check_both(value: Old):
    if isinstance(value, AlsoOld):
        value.value  # error: [deprecated] "`Old.value`, `AlsoOld.value`: old getter"
        value.value = 1  # error: [deprecated] "`Old.value`, `AlsoOld.value`: old setter"
        del value.value  # error: [deprecated] "`Old.value`, `AlsoOld.value`: old deleter"
```

A non-deprecated getter suppresses the warning on reads. Assignments still warn when both setters
are deprecated.

```py
class ActiveGetter:
    @property
    def value(self) -> int:
        return 0

    @value.setter
    @deprecated("old setter")
    def value(self, value: int) -> None: ...

def check_active_getter(value: Old):
    if isinstance(value, ActiveGetter):
        value.value
        value.value = 1  # error: [deprecated] "`Old.value`, `ActiveGetter.value`: old setter"
```

## Invalid property getter calls

The getter below accepts an `int`, but Python passes a `C` instance. Reading the property reports
both the invalid call and the deprecation.

```py
from typing_extensions import deprecated

class C:
    @property
    @deprecated("invalid getter")
    def value(self: int) -> int:
        return self

# error: [invalid-attribute-access]
# error: [deprecated] "invalid getter"
C().value
```

## Setter deprecations and contextual inference

The ordinary attribute supplies `int` as the lambda's parameter type. Checking the other member's
deprecated setter does not change that inferred type or warn about the non-deprecated assignment.

```py
from collections.abc import Callable
from typing_extensions import deprecated

class Ordinary:
    callback: Callable[[int], object]

class Deprecated:
    @property
    def callback(self) -> object: ...
    @callback.setter
    @deprecated("old setter")
    def callback(self, value: Callable[[str], object]) -> None: ...

def check(value: Ordinary):
    if isinstance(value, Deprecated):
        value.callback = lambda argument: reveal_type(argument)  # revealed: int
```

Deprecation checks also preserve the `str` parameter type inferred from a protocol setter.

```py
from typing import Protocol

class Callback(Protocol):
    @property
    def callback(self) -> Callable[[str], object]: ...
    @callback.setter
    @deprecated("callback setter")
    def callback(self, value: Callable[[str], object]) -> None: ...

def check_protocol(value: Callback):
    if isinstance(value, Ordinary):
        value.callback = lambda argument: reveal_type(argument)  # revealed: str
```

## Overloads

### Deprecated overloads

A call reports the deprecation of the overload selected by its arguments.

```py
from typing_extensions import deprecated, overload

@overload
@deprecated("strings are no longer supported")
def f(x: str): ...
@overload
def f(x: int): ...
def f(x):
    print(x)

f(1)
f("hello")  # error: [deprecated] "strings are no longer supported"
```

Referring to the function without calling it does not select an overload and does not warn.

```py
f
```

### Deprecated implementations

A deprecated implementation makes every call deprecated. Its message takes precedence over an
individual overload's deprecation, and a call produces only one warning.

```py
from typing_extensions import deprecated, overload

@overload
@deprecated("string overload")
def f(x: str): ...
@overload
def f(x: int): ...
@deprecated("entire function")
def f(x):
    print(x)

f(1)  # error: [deprecated] "entire function"
f("hello")  # error: [deprecated] "entire function"
```

### Equivalent return types

An `Any` argument matches both overloads below. Their return types are equivalent, so overload
resolution selects the first. The deprecated second overload does not produce a warning.

```py
from typing import Any, overload
from typing_extensions import deprecated

@overload
def convert(value: int) -> str: ...
@overload
@deprecated("strings are no longer supported")
def convert(value: str) -> str: ...
def convert(value: int | str) -> str:
    return str(value)

def check(value: Any):
    convert(value)
```

### Ambiguous overloads

All three overloads remain possible for an `Any` argument. Their return types differ, so no single
overload wins. The call reports the deprecated overload that remains a possible target.

```py
from typing import Any, overload
from typing_extensions import deprecated

@overload
def convert(value: list[int]) -> int: ...
@overload
@deprecated("string lists are no longer supported")
def convert(value: list[str]) -> str: ...
@overload
def convert(value: bytes) -> bytes: ...
def convert(value): ...
def check(value: Any):
    convert(value)  # error: [deprecated] "string lists are no longer supported"
```

The same ambiguity can occur within one member of a union. The `list[Any]` member can select the
deprecated overload, even though the `bytes` member selects a non-deprecated overload.

```py
def check_union(value: list[Any] | bytes):
    convert(value)  # error: [deprecated] "string lists are no longer supported"
```

### Union arguments

A union argument can select different overloads for different members. The call reports a
deprecation if any selected overload is deprecated.

```py
from typing import overload
from typing_extensions import deprecated

@overload
def convert(value: int) -> str: ...
@overload
@deprecated("use an integer")
def convert(value: str) -> str: ...
def convert(value: int | str) -> str:
    return str(value)

def check(value: int | str):
    convert(value)  # error: [deprecated] "use an integer"
```

If no overload accepts the argument, there is no selected overload to report as deprecated.

```py
convert(None)  # error: [no-matching-overload]
```

### Equivalent return types within a union

Each member of a union resolves its overloads separately. The `list[Any]` member matches the first
two overloads, whose equivalent return types select the first. The `bytes` member selects the third
overload. Neither selected overload is deprecated.

```py
from typing import Any, overload
from typing_extensions import deprecated

@overload
def convert(value: list[int]) -> str: ...
@overload
@deprecated("string lists are no longer supported")
def convert(value: list[str]) -> str: ...
@overload
def convert(value: bytes) -> str: ...
def convert(value) -> str:
    return ""

def check(value: list[Any] | bytes):
    convert(value)
```

### Ambiguity in one union member

Ambiguity in one union member does not change how another member selects its overload. The
`list[Any]` member can select the deprecated overload for lists of strings. The `set[Any]` member's
two overloads have equivalent return types, so only its non-deprecated first overload is selected.

```py
from typing import Any, overload
from typing_extensions import deprecated

@overload
def convert(value: list[int]) -> int: ...
@overload
@deprecated("string lists are no longer supported")
def convert(value: list[str]) -> str: ...
@overload
def convert(value: set[int]) -> int: ...
@overload
@deprecated("string sets are no longer supported")
def convert(value: set[str]) -> int: ...
def convert(value): ...
def check(value: list[Any] | set[Any]):
    # error: [deprecated] "The overload of `convert` is deprecated: string lists are no longer supported"
    convert(value)
```

### Calls that select several deprecated overloads

An `int | str` argument selects a different overload for each member of the union. When both
overloads are deprecated, the warning names the function once.

```py
from typing import overload
from typing_extensions import deprecated

@overload
@deprecated("integer overload")
def convert(value: int) -> str: ...
@overload
@deprecated("string overload")
def convert(value: str) -> str: ...
def convert(value: int | str) -> str:
    return str(value)

def check(value: int | str):
    # error: [deprecated] "Possible use of deprecated function: `convert`"
    convert(value)
```

### Shared deprecation messages across overloads

When selected overloads share a deprecation message, the warning includes that message once in both
full and concise output. The full diagnostic points to each deprecated overload.

```py
from typing import overload
from typing_extensions import deprecated

@overload
@deprecated("Use `parse` instead. Support ends in version 2.")
def convert(value: int) -> str: ...
@overload
@deprecated("Use `parse` instead. Support ends in version 2.")
def convert(value: str) -> str: ...
def convert(value: int | str) -> str:
    return str(value)

def check(value: int | str):
    # snapshot: deprecated
    convert(value)
```

```snapshot
warning[deprecated]: Possible use of deprecated function: `convert`
  --> src/mdtest_snippet.py:15:5
   |
15 |     convert(value)
   |     ^^^^^^^ Use `parse` instead. Support ends in version 2.
   |
  ::: src/mdtest_snippet.py:6:5
   |
 6 | def convert(value: int) -> str: ...
   |     -------
 7 | @overload
 8 | @deprecated("Use `parse` instead. Support ends in version 2.")
 9 | def convert(value: str) -> str: ...
   |     -------
```

### Overloads for different receivers

The `self` annotations restrict which overloads each instance can call. A call on `C[str]` reports
the deprecated string overload, even though binding the method discards the integer overload.

```py
from typing import Generic, TypeVar, overload
from typing_extensions import deprecated

T = TypeVar("T")

class C(Generic[T]):
    @overload
    def method(self: "C[int]", value: int) -> int: ...
    @overload
    @deprecated("string method")
    def method(self: "C[str]", value: str) -> str: ...
    def method(self, value: int | str) -> int | str:
        return value

def check(integer: C[int], string: C[str]):
    integer.method(1)
    string.method("one")  # error: [deprecated] "string method"
```

## Deprecated constructors

Calling a class can invoke `__new__`, `__init__`, or a custom metaclass's `__call__`. We warn about
deprecated methods that the call invokes, even when the class itself is not deprecated.

### `__init__`

Calling a class reports the deprecation of its initializer.

```py
from typing_extensions import Self, deprecated

class OldInit:
    @deprecated("old init")
    def __init__(self) -> None: ...

OldInit()  # error: [deprecated] "old init"
```

An explicit call on an instance also produces only one warning.

```py
def explicit_init(value: OldInit):
    value.__init__()  # error: [deprecated] "old init"
```

If a non-deprecated `__new__` returns an instance of the class, Python still calls the inherited
deprecated initializer.

```py
class NewWithOldInit(OldInit):
    def __new__(cls) -> Self:
        return super().__new__(cls)

NewWithOldInit()  # error: [deprecated] "old init"
```

When `__new__` returns an unrelated type, `__init__` does not run and produces no warning.

```py
class ReturnsInt(OldInit):
    def __new__(cls) -> int:
        return 0

ReturnsInt()
```

### `__new__`

Calling a class also reports the deprecation of the method that creates the instance.

```py
from typing_extensions import Self, deprecated

class OldNew:
    @deprecated("old new")
    def __new__(cls) -> Self:
        return super().__new__(cls)

OldNew()  # error: [deprecated] "old new"
```

Calling `__new__` explicitly produces only one warning.

```py
OldNew.__new__(OldNew)  # error: [deprecated] "old new"
```

### Both constructor methods

When both methods are deprecated, the class call produces one warning that points to both
declarations and includes both messages.

```py
from typing_extensions import Self, deprecated

class Both:
    @deprecated("old new")
    def __new__(cls) -> Self:
        return super().__new__(cls)

    @deprecated("old init")
    def __init__(self) -> None: ...

# snapshot: deprecated
Both()
```

```snapshot
warning[deprecated]: Possible use of deprecated methods: `Both.__new__`, `Both.__init__`
  --> src/mdtest_snippet.py:12:1
   |
12 | Both()
   | ^^^^
info: old new
 --> src/mdtest_snippet.py:5:9
  |
5 |     def __new__(cls) -> Self:
  |         ^^^^^^^
info: old init
 --> src/mdtest_snippet.py:9:9
  |
9 |     def __init__(self) -> None: ...
  |         ^^^^^^^^
```

### Metaclass `__call__`

Calling a class with a custom metaclass invokes the metaclass's `__call__` method. If that method is
deprecated, the class call produces a warning.

```py
from typing_extensions import deprecated

class Meta(type):
    @deprecated("metaclass call")
    def __call__(cls) -> int:
        return 0

class WithMeta(metaclass=Meta): ...

WithMeta()  # error: [deprecated] "metaclass call"
```

## Calls to an inherited deprecated method

When both union members inherit the same deprecated method, a call reports that method once.

```py
from typing_extensions import deprecated

class Base:
    @deprecated("base call")
    def __call__(self) -> None: ...

class First(Base): ...
class Second(Base): ...

def check(value: First | Second):
    value()  # error: [deprecated] "base call"
```

Separate calls each produce a warning, even when they are on the same line.

```py
def two_calls(value: First | Second):
    # error: 6 [deprecated] "base call"
    # error: 15 [deprecated] "base call"
    (value(), value())
```

## Calls to an inherited deprecated overload

A deprecated overload inherited by both members of a union also produces only one warning per call.

```py
from typing import overload
from typing_extensions import deprecated

class Overloaded:
    @overload
    @deprecated("integer call")
    def __call__(self, value: int) -> None: ...
    @overload
    def __call__(self, value: str) -> None: ...
    def __call__(self, value: int | str) -> None: ...

class FirstOverload(Overloaded): ...
class SecondOverload(Overloaded): ...

def check_overload(value: FirstOverload | SecondOverload):
    value(1)  # error: [deprecated] "integer call"
    value("one")
```

## Suppressing call deprecations

An inline ignore suppresses the deprecation on an instance's implicit `__call__` invocation.

```py
from typing_extensions import deprecated

class Callable:
    @deprecated("do not call")
    def __call__(self) -> None: ...

Callable()()  # ty: ignore[deprecated]
```

An unreachable call does not produce a warning.

```py
if False:
    Callable()()
```

The same applies to calls inside a `no_type_check` function.

```py
from typing import no_type_check

@no_type_check
def unchecked(value: Callable):
    value()
```

## Repeated binary operations

Several combinations of union members can invoke the same deprecated operator. Each expression
reports that method once. Repeating the operation still produces a warning at the second expression.

```py
from typing_extensions import Self, deprecated

class Number:
    @deprecated("addition")
    def __add__(self, other: int | str) -> Self:
        return self

class First(Number): ...
class Second(Number): ...

def check(number: First | Second, value: int | str):
    number + value  # error: [deprecated] "addition"
    number + value  # error: [deprecated] "addition"
```

The same rule applies when augmented assignment falls back to `__add__`.

```py
def check_augmented(number: First | Second, value: int | str):
    number += value  # error: [deprecated] "addition"
```

When some members provide `__iadd__` and others fall back to `__add__`, the warning includes the
deprecations of both methods.

```py
class InPlace(Number):
    @deprecated("in-place addition")
    def __iadd__(self, other: int | str) -> Self:
        return self

def check_mixed(number: First | InPlace, value: int | str):
    # error: [deprecated] "`Number.__add__`, `InPlace.__iadd__`"
    number += value
```

## Calls to different deprecated methods

Deprecation messages can contain several sentences, semicolons, and line breaks.

```py
from typing_extensions import deprecated

class First:
    @deprecated("Use `invoke` instead. Direct calls are deprecated; support ends in version 2.")
    def __call__(self) -> None: ...

class Second:
    @deprecated("Use `invoke` instead.\nSupport ends in version 3.")
    def __call__(self) -> None: ...
```

When either method can be called, the warning names both defining classes. The full diagnostic shows
each message beside its method's definition, preserving its punctuation and line breaks.

```py
def check(value: First | Second):
    # snapshot: deprecated
    value()
```

```snapshot
warning[deprecated]: Possible use of deprecated methods: `First.__call__`, `Second.__call__`
  --> src/mdtest_snippet.py:12:5
   |
12 |     value()
   |     ^^^^^
info: Use `invoke` instead. Direct calls are deprecated; support ends in version 2.
 --> src/mdtest_snippet.py:5:9
  |
5 |     def __call__(self) -> None: ...
  |         ^^^^^^^^
info: Use `invoke` instead.
Support ends in version 3.
 --> src/mdtest_snippet.py:9:9
  |
9 |     def __call__(self) -> None: ...
  |         ^^^^^^^^
```

## Calls to deprecated generic methods

The warning includes the defining class's name even when both the class and method have type
parameters.

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import deprecated

class First[T]:
    @deprecated("first method")
    def __call__[U](self, value: U) -> U:
        return value

class Second:
    @deprecated("second method")
    def __call__(self, value: int) -> int:
        return value

def check(value: First[int] | Second):
    # error: [deprecated] "Possible use of deprecated methods: `First.__call__`, `Second.__call__`"
    value(1)
```
