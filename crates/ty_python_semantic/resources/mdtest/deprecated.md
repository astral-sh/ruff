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

## Decorator order

`@deprecated` applies to the result of any inner decorators. If an inner decorator replaces a
function with a different function, the public binding should be deprecated without marking the
replacement function itself as deprecated.

```py
from collections.abc import Callable
from typing import Any, TypeVar, cast, overload
from ty_extensions import TypeOf, is_equivalent_to, static_assert
from typing_extensions import TypeGuard, deprecated

R = TypeVar("R")

def replacement() -> str:
    return "replacement"

def other() -> str:
    return "other"

@deprecated("deprecated replacement")
def deprecated_replacement() -> str:
    return "deprecated replacement"

def replace_with(replacement: R) -> Callable[[Callable[..., Any]], R]:
    raise NotImplementedError

def is_replacement(value: object) -> TypeGuard[TypeOf[replacement]]:
    return True

def is_callable(value: object) -> TypeGuard[Callable[..., Any]]:
    return True

class ReplacementClass: ...

class CallableReplacement:
    def __call__(self) -> str:
        return "replacement"

callable_replacement = CallableReplacement()

@deprecated("ordinary deprecated function")
def ordinary_deprecated_function() -> None: ...

def deprecated_factory() -> TypeOf[ordinary_deprecated_function]:  # ty: ignore[deprecated]
    return ordinary_deprecated_function  # ty: ignore[deprecated]

def is_ordinary_deprecated(
    value: object,
) -> TypeGuard[TypeOf[ordinary_deprecated_function]]:  # ty: ignore[deprecated]
    return True

@deprecated("use replacement directly")
@replace_with(replacement)
def deprecated_binding() -> None: ...
@replace_with(replacement)
@deprecated("only the replaced function is deprecated")
def replaced_deprecated_function() -> None: ...
@deprecated("use replacement or other directly")
@replace_with(cast(TypeOf[replacement] | TypeOf[other], replacement))
def deprecated_union_binding() -> None: ...
@deprecated("outer deprecation")
@replace_with(replacement)
@deprecated("inner deprecation")
def deprecated_outer_binding() -> None: ...
@deprecated("this object is not callable")
@replace_with(object())
def deprecated_object_binding() -> None: ...
@deprecated("callable replacement")
@replace_with(cast(Callable[[], str], replacement))
def deprecated_callable_binding() -> None: ...
@deprecated("use ReplacementClass directly")
@replace_with(ReplacementClass)
def deprecated_class_binding() -> None: ...
@deprecated("use callable_replacement directly")
@replace_with(callable_replacement)
def deprecated_callable_instance_binding() -> None: ...
@deprecated("overload binding")
@overload
@replace_with(deprecated_replacement)  # error: [deprecated] "deprecated replacement"
def deprecated_overload_binding() -> str: ...

deprecated_binding()  # error: [deprecated] "use replacement directly"
replacement()
replaced_deprecated_function()
deprecated_union_binding  # error: [deprecated] "use replacement or other directly"
deprecated_outer_binding()  # error: [deprecated] "outer deprecation"
deprecated_object_binding
deprecated_callable_binding()  # error: [deprecated] "callable replacement"
deprecated_class_binding  # error: [deprecated] "use ReplacementClass directly"
deprecated_callable_instance_binding()  # error: [deprecated] "use callable_replacement directly"
callable_replacement()
deprecated_overload_binding()  # error: [deprecated] "deprecated replacement"

copy_of_deprecated_binding = deprecated_binding  # error: [deprecated] "use replacement directly"
copy_of_deprecated_binding()

static_assert(is_equivalent_to(TypeOf[deprecated_binding], TypeOf[replacement]))  # error: [deprecated]

if deprecated_binding is not replacement:  # error: [deprecated] "use replacement directly"
    deprecated_binding()

if is_replacement(ordinary_deprecated_function):  # error: [deprecated] "ordinary deprecated function"
    ordinary_deprecated_function()

if is_callable(deprecated_binding):  # error: [deprecated] "use replacement directly"
    deprecated_binding()  # error: [deprecated] "use replacement directly"

flag = bool(input())
if flag:
    @deprecated("use replacement directly")
    @replace_with(replacement)
    def narrowed_binding() -> None: ...
else:
    narrowed_binding = other

if is_replacement(narrowed_binding):
    narrowed_binding()  # error: [deprecated] "use replacement directly"

if flag:
    chained_source = ordinary_deprecated_function  # error: [deprecated] "ordinary deprecated function"
elif bool(input()):
    chained_source = deprecated_factory()
else:
    chained_source = replacement

chained_alias = chained_source
if is_ordinary_deprecated(chained_alias):
    chained_alias()  # error: [deprecated] "ordinary deprecated function"

if flag:
    @deprecated("use replacement directly")
    @replace_with(replacement)
    def conditionally_defined() -> None: ...

else:
    @deprecated("use other directly")
    @replace_with(other)
    def conditionally_defined() -> None: ...

conditionally_defined()  # TODO: error: [deprecated]

if flag:
    @deprecated("same message")
    @replace_with(replacement)
    def same_message() -> None: ...

else:
    @deprecated("same message")
    @replace_with(other)
    def same_message() -> None: ...

same_message()  # error: [deprecated] "same message"

@deprecated("loop binding")
@replace_with(replacement)
def loop_binding() -> None: ...

for _ in range(1):
    loop_binding()  # error: [deprecated] "loop binding"

    @deprecated("loop binding")
    @replace_with(replacement)
    def loop_binding() -> None: ...

mixed_loop_binding = other
for _ in range(1):
    if is_replacement(mixed_loop_binding):
        mixed_loop_binding()  # error: [deprecated] "mixed loop binding"

    if flag:
        @deprecated("mixed loop binding")
        @replace_with(replacement)
        def mixed_loop_binding() -> None: ...
    else:
        mixed_loop_binding = other

def nested_binding():
    @deprecated("nested binding")
    @replace_with(replacement)
    def old() -> None: ...
    def rebind():
        nonlocal old

        @deprecated("nested binding")
        @replace_with(replacement)
        def old() -> None: ...

    old()  # error: [deprecated] "nested binding"

def mixed_nested_binding():
    old = other

    def rebind():
        nonlocal old
        if flag:
            @deprecated("mixed nested binding")
            @replace_with(replacement)
            def old() -> None: ...
        else:
            old = other

    if is_replacement(old):
        old()  # error: [deprecated] "mixed nested binding"

if True:
    @deprecated("reachable binding")
    @replace_with(replacement)
    def statically_reachable() -> None: ...

else:
    @deprecated("unreachable binding")
    @replace_with(other)
    def statically_reachable() -> None: ...

statically_reachable()  # error: [deprecated] "reachable binding"

class C:
    def replacement(self) -> str:
        return "replacement"

    @deprecated("use replacement directly")
    @replace_with(replacement)
    def deprecated_binding(self) -> None: ...

C.deprecated_binding  # error: [deprecated] "use replacement directly"
C().deprecated_binding  # error: [deprecated] "use replacement directly"

class D(C): ...
class E(C): ...

T = TypeVar("T", D, E)

def use_union(instance: D | E) -> None:
    instance.deprecated_binding  # error: [deprecated] "use replacement directly"

def use_typevar(instance: T) -> None:
    instance.deprecated_binding  # error: [deprecated] "use replacement directly"
```

If control flow exposes multiple live decorated bindings with different deprecation messages, there
is no single message to report.

## Imported decorated binding

`module.py`:

```py
from collections.abc import Callable
from typing import Any, TypeVar
from typing_extensions import deprecated

F = TypeVar("F", bound=Callable[..., Any])

def replacement() -> str:
    return "replacement"

def replace_with(replacement: F) -> Callable[[Callable[..., Any]], F]:
    raise NotImplementedError

@deprecated("use replacement directly")
@replace_with(replacement)
def deprecated_binding() -> None: ...
```

`main.py`:

```py
import module

# error: [deprecated] "use replacement directly"
from module import deprecated_binding

deprecated_binding()  # error: [deprecated] "use replacement directly"
module.deprecated_binding  # error: [deprecated] "use replacement directly"
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
from collections.abc import Callable
from typing import Any, TypeAlias
from ty_extensions import TypeOf
from typing_extensions import TypeGuard, deprecated

@deprecated("Use OtherType instead")
class DeprType: ...

@deprecated("Use other_func instead")
def depr_func(): ...

class C:
    @deprecated("Use other_method instead")
    def depr_method(self): ...

alias_func = depr_func  # error: [deprecated] "Use other_func instead"
AliasClass = DeprType  # error: [deprecated] "Use OtherType instead"
alias_method = C().depr_method  # error: [deprecated] "Use other_method instead"

alias_func()
AliasClass()
alias_method()

second_alias = alias_func
second_alias()

annotated_alias: Callable[..., Any] = depr_func  # error: [deprecated] "Use other_func instead"
annotated_alias()

separate_alias: TypeOf[depr_func]  # ty: ignore[deprecated]
separate_alias = depr_func  # error: [deprecated] "Use other_func instead"
separate_alias()

ExplicitAlias: TypeAlias = DeprType  # error: [deprecated] "Use OtherType instead"
explicit_value: ExplicitAlias

(named_alias := depr_func)  # error: [deprecated] "Use other_func instead"
named_alias()

(inline_alias := depr_func)()  # error: [deprecated] "Use other_func instead"
inline_alias()

(unpacked_alias,) = (depr_func,)  # error: [deprecated] "Use other_func instead"
unpacked_alias()

def union_alias(flag: bool):
    optional_alias = depr_func if flag else None  # error: [deprecated] "Use other_func instead"
    if optional_alias is not None:
        optional_alias()

def branch_alias(flag: bool):
    if flag:
        branch_value = depr_func  # error: [deprecated] "Use other_func instead"
    else:
        branch_value = None
    if branch_value is not None:
        branch_value()

def factory() -> TypeOf[depr_func]:  # ty: ignore[deprecated]
    return depr_func  # ty: ignore[deprecated]

constant_alias = depr_func if True else factory()  # error: [deprecated] "Use other_func instead"
constant_alias()

factory_alias = factory()
factory_alias()  # error: [deprecated] "Use other_func instead"

selected_factory_alias = (depr_func, factory())[1]  # error: [deprecated] "Use other_func instead"
selected_factory_alias()  # error: [deprecated] "Use other_func instead"

starred_factory_alias = (*[factory(), factory()], depr_func)[1]  # error: [deprecated] "Use other_func instead"
# TODO: Preserve deprecated function-literal identity through starred tuple inference.
starred_factory_alias()

first_alias, unpacked_factory_alias = depr_func, factory()  # error: [deprecated] "Use other_func instead"
first_alias()
unpacked_factory_alias()  # error: [deprecated] "Use other_func instead"

def mixed_alias(flag: bool):
    if flag:
        mixed_value = depr_func  # error: [deprecated] "Use other_func instead"
    else:
        mixed_value = factory()
    mixed_value()  # error: [deprecated] "Use other_func instead"

def is_depr_func(value: object) -> TypeGuard[TypeOf[depr_func]]:  # ty: ignore[deprecated]
    return True

def narrowed_alias(flag: bool, unknown: object):
    if flag:
        narrowed_value = depr_func  # error: [deprecated] "Use other_func instead"
    else:
        narrowed_value = unknown
    if is_depr_func(narrowed_value):
        narrowed_value()  # error: [deprecated] "Use other_func instead"

def narrowed_union_alias(value: TypeOf[depr_func] | None):  # ty: ignore[deprecated]
    union_value = value
    if union_value is not None:
        union_value()  # error: [deprecated] "Use other_func instead"

def loop_alias():
    alias = depr_func  # error: [deprecated] "Use other_func instead"
    for _ in range(1):
        alias()
        alias = depr_func  # error: [deprecated] "Use other_func instead"

def nested_alias():
    alias = depr_func  # error: [deprecated] "Use other_func instead"

    def rebind():
        nonlocal alias
        alias = depr_func  # error: [deprecated] "Use other_func instead"

    alias()

# TODO: Attribute and loop-target assignments should also avoid transitive deprecation.
class AliasHolder:
    alias: Callable[..., Any]

holder = AliasHolder()
holder.alias = depr_func  # error: [deprecated] "Use other_func instead"
holder.alias()  # error: [deprecated] "Use other_func instead"

for loop_target_alias in (depr_func,):  # error: [deprecated] "Use other_func instead"
    loop_target_alias()  # error: [deprecated] "Use other_func instead"
```

## Dunders

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
z = x + y  # TODO error: [deprecated] "MyInt `+` support is broken"
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
f("hello")  # TODO: error: [deprecated] "strings are no longer supported"
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
