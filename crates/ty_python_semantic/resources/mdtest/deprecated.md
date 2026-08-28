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

If a dunder like `__add__` is deprecated, then the equivalent syntactic sugar like `+` should fire a
diagnostic.

```py
from typing_extensions import deprecated

class MyInt:
    def __init__(self, val):
        self.val = val

    @deprecated("MyInt `+` support is broken")
    def __add__(self, other):
        return MyInt(self.val + other.val)

x = MyInt(1)
y = MyInt(2)
z = x + y  # error: [deprecated] "MyInt `+` support is broken"
x += y  # error: [deprecated] "MyInt `+` support is broken"
```

### Reflected operators

Only the methods selected by operator resolution trigger deprecations. A deprecated reflected method
is not used when the left operand accepts the operation.

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
1 + Right()  # error: [deprecated] "reflected addition"
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

Augmented assignment calls the in-place method when available, without reporting deprecations on the
unused binary method.

```py
from typing_extensions import deprecated

class Number:
    @deprecated("binary addition")
    def __add__(self, other: int) -> "Number":
        return self

    def __iadd__(self, other: int) -> "Number":
        return self

class OldNumber:
    @deprecated("in-place addition")
    def __iadd__(self, other: int) -> "OldNumber":
        return self

number = Number()
number += 1
old = OldNumber()
old += 1  # error: [deprecated] "in-place addition"
```

### Callable instances

Calling an instance implicitly invokes its `__call__` method. Explicit references to that method
already report its deprecation and do not produce a second diagnostic when called.

```py
from typing_extensions import deprecated

class Invocable:
    @deprecated("do not call")
    def __call__(self) -> int:
        return 0

invocable = Invocable()
invocable()  # error: [deprecated] "do not call"
invocable.__call__()  # error: [deprecated] "do not call"
invocable.__call__  # error: [deprecated] "do not call"
```

For overloaded `__call__` methods, only calls that select a deprecated overload trigger a warning.

```py
from typing import overload

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

A union reports a deprecated operator when any alternative is deprecated. An intersection reports
deprecated operators only when every applicable implementation is deprecated.

```py
from typing_extensions import deprecated

class Deprecated:
    @deprecated("old inversion")
    def __invert__(self) -> int:
        return 1

class AlsoDeprecated:
    @deprecated("another old inversion")
    def __invert__(self) -> int:
        return 2

class Ordinary:
    def __invert__(self) -> int:
        return 3

def mixed_union(value: Deprecated | Ordinary) -> None:
    ~value  # error: [deprecated] "old inversion"

def mixed_intersection(value: Deprecated) -> None:
    if isinstance(value, Ordinary):
        ~value

def deprecated_intersection(value: Deprecated) -> None:
    if isinstance(value, AlsoDeprecated):
        # error: [deprecated] "old inversion; another old inversion"
        ~value
```

A gradually typed comparison can produce an intersection of `bool` and `Any`. The unknown
alternative might provide a nondeprecated operator, so inverting it should not warn.

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

Type variable constraints also should be checked.

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
    # error: [deprecated] "first; second"
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
    # error: [deprecated] "first; second"
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
    # error: [deprecated] "invalid inversion; second"
    ~value
```

## Property accessors

Only the accessor invoked by an operation triggers its deprecation. Reading a property does not
invoke its setter, writing does not invoke its getter, and deleting does not invoke either.
Augmented assignment reads and then writes the property.

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
old_getter.value += 1  # error: [deprecated] "old getter"
del old_getter.value
OldGetter.value

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
old_setter.value += 1  # error: [deprecated] "old setter"
del old_setter.value  # error: [deprecated] "old deleter"
OldSetter.value
```

When both accessors are deprecated, augmented assignment reports both messages.

```py
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

Inherited accessors retain their deprecations. An override can replace a deprecated getter, while
access through `super()` still invokes the inherited getter. Class-bound `super()` returns the
property object without invoking its getter.

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

class ActiveChild(Parent):
    @property
    def value(self) -> int:
        return super().value  # error: [deprecated] "parent getter"

Child().value  # error: [deprecated] "parent getter"
ActiveChild().value
super(Child, Child).value
```

Deleting through an instance invokes the inherited deleter. `super()` delegates reads to the owner's
descriptors, but it does not delegate deletion.

```py
del Child().value  # error: [deprecated] "parent deleter"
del super(Child, Child()).value

class Ordinary:
    value: int

def delete_union(flag: bool):
    target = super(Child, Child()) if flag else Ordinary()
    del target.value
```

## Metaclass properties

A class is an instance of its metaclass, so reading or writing a metaclass property invokes its
accessors. Access through the metaclass itself returns the property object.

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
Meta.value
```

## Properties on unions

An access through a union can invoke a deprecated accessor even when another member's accessor is
not deprecated.

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
    value: int = 0

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

An intersection can obtain its implementation from a non-deprecated member. We do not warn when that
member provides an alternative getter or setter.

```py
def check_intersection(value: Old):
    if isinstance(value, Active):
        value.value
        value.value = 1
```

An intersection member without the attribute does not provide an alternative accessor, so the
deprecated property still applies.

```py
class Marker: ...

def check_marker(value: Old):
    if isinstance(value, Marker):
        value.value  # error: [deprecated] "union getter"
        value.value = 1  # error: [deprecated] "union setter"
```

An invalid assignment to the first intersection in a union does not hide a deprecated setter in the
second intersection. A member without the attribute still provides no alternative setter.

```py
def check_invalid_intersections(value: Wrong | Old):
    if isinstance(value, Marker):
        # error: [invalid-assignment]
        # error: [deprecated] "union setter"
        value.value = 1
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

An intersection can use an ordinary attribute annotation to infer a lambda's parameter type.
Checking the other member's deprecated setter does not replace that inference context, and the
ordinary attribute provides a non-deprecated alternative.

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

A protocol setter can also provide the inference context. When that setter accepts the assignment
first, the ordinary attribute suppresses its deprecation without changing the inferred parameter
type.

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

Overloads can be deprecated, but only trigger warnings when invoked.

```py
from typing_extensions import deprecated
from typing_extensions import overload

@overload
@deprecated("strings are no longer supported")
def f(x: str): ...
@overload
def f(x: int): ...
def f(x):
    print(x)

f(1)
f("hello")  # error: [deprecated] "strings are no longer supported"
f  # Referring to the overloaded function does not select an overload.
```

If the actual impl is deprecated, the deprecation always fires.

```py
from typing_extensions import deprecated
from typing_extensions import overload

@overload
def f(x: str): ...
@overload
def f(x: int): ...
@deprecated("unusable")
def f(x):
    print(x)

f(1)  # error: [deprecated] "unusable"
f("hello")  # error: [deprecated] "unusable"
```

## Overload selection

Calls with union arguments can select multiple overloads. We report a deprecation if one of those
overloads is deprecated, but do not report one when no overload accepts the arguments.

```py
from typing import overload
from typing_extensions import deprecated

@overload
@deprecated("use a string")
def convert(value: int) -> str: ...
@overload
def convert(value: str) -> str: ...
def convert(value: int | str) -> str:
    return str(value)

def check(value: int | str):
    convert(value)  # error: [deprecated] "use a string"

convert(1)  # error: [deprecated] "use a string"
convert("one")
convert(None)  # error: [no-matching-overload]
```

## Specialized receivers

Binding a method can remove overloads whose receiver annotations are incompatible with the instance.
Deprecation messages still refer to the selected source overload.

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

## Deprecated overload and implementation

When both the implementation and a selected overload are deprecated, the reference to the function
already reports the implementation's deprecation. We do not emit a second diagnostic for the call.

```py
from typing import overload
from typing_extensions import deprecated

@overload
@deprecated("integer overload")
def convert(value: int) -> str: ...
@overload
def convert(value: str) -> str: ...
@deprecated("entire function")
def convert(value: int | str) -> str:
    return str(value)

convert(1)  # error: [deprecated] "entire function"
convert("one")  # error: [deprecated] "entire function"
```

## Repeated inherited deprecations

Both union alternatives can inherit the same deprecated method. Calling it reports that source
method once at each call site, rather than once per possible receiver.

```py
from typing_extensions import deprecated

class Base:
    @deprecated("base call")
    def __call__(self) -> None: ...

class First(Base): ...
class Second(Base): ...

def check(value: First | Second):
    value()  # error: [deprecated] "base call"
    value()  # error: [deprecated] "base call"

    # error: [deprecated] "base call"
    # error: [deprecated] "base call"
    (value(), value())

    value()  # ty: ignore[deprecated]
    if False:
        value()
```

The same applies to an inherited overload selected through multiple possible receivers.

```py
from typing import overload

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

Calls inside a `no_type_check` function remain silent.

```py
from typing import no_type_check

@no_type_check
def unchecked(value: First | Second):
    value()
```

## Repeated binary deprecations

Expanding union operands can select the same deprecated operator repeatedly. The expression reports
the method once, even when both operands are unions.

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

Augmented assignment also collects the selected methods across union alternatives, including a
fallback from `__iadd__` to `__add__`.

```py
class InPlace(Number):
    @deprecated("in-place addition")
    def __iadd__(self, other: int | str) -> Self:
        return self

def check_augmented(number: First | Second, value: int | str):
    number += value  # error: [deprecated] "addition"

def check_mixed(number: First | InPlace, value: int | str):
    number += value  # error: [deprecated] "addition; in-place addition"
```

## Multiple deprecated targets

Distinct deprecated implementations remain visible in a single diagnostic. Each source declaration
is annotated once, even when several union alternatives inherit it.

```py
from typing_extensions import deprecated

class First:
    @deprecated("first message")
    def __call__(self) -> None: ...

class Second:
    @deprecated("second message")
    def __call__(self) -> None: ...

class Inherited(First): ...

def check(value: First | Second | Inherited):
    # snapshot: deprecated
    value()
```

```snapshot
warning[deprecated]: Use of deprecated functions
  --> src/mdtest_snippet.py:15:5
   |
15 |     value()
   |     ^^^^^ first message; second message
   |
  ::: src/mdtest_snippet.py:5:9
   |
 5 |     def __call__(self) -> None: ...
   |         -------- first message
 6 |
 7 | class Second:
 8 |     @deprecated("second message")
 9 |     def __call__(self) -> None: ...
   |         -------- second message
```
