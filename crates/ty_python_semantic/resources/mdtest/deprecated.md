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
