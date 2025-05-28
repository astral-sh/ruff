# Unreachable code

This document describes our approach to handling unreachable code. There are two aspects to this.
One is to detect and mark blocks of code that are unreachable. This is useful for notifying the
user, as it can often be indicative of an error. The second aspect of this is to make sure that we
do not emit (incorrect) diagnostics in unreachable code.

## Detecting unreachable code

In this section, we look at various scenarios how sections of code can become unreachable. We should
eventually introduce a new diagnostic that would detect unreachable code. In an editor/LSP context,
there are ways to 'gray out' sections of code, which is helpful for blocks of code that are not
'dead' code, but inactive under certain conditions, like platform-specific code.

### Terminal statements

In the following examples, the `print` statements are definitely unreachable.

```py
def f1():
    return

    # TODO: we should mark this as unreachable
    print("unreachable")

def f2():
    raise Exception()

    # TODO: we should mark this as unreachable
    print("unreachable")

def f3():
    while True:
        break

        # TODO: we should mark this as unreachable
        print("unreachable")

def f4():
    for _ in range(10):
        continue

        # TODO: we should mark this as unreachable
        print("unreachable")
```

### Infinite loops

```py
def f1():
    while True:
        pass

    # TODO: we should mark this as unreachable
    print("unreachable")
```

### Statically known branches

In the following examples, the `print` statements are also unreachable, but it requires type
inference to determine that:

```py
def f1():
    if 2 + 3 > 10:
        # TODO: we should mark this as unreachable
        print("unreachable")

def f2():
    if True:
        return

    # TODO: we should mark this as unreachable
    print("unreachable")
```

### `Never` / `NoReturn`

If a function is annotated with a return type of `Never` or `NoReturn`, we can consider all code
after the call to that function unreachable.

```py
from typing_extensions import NoReturn

def always_raises() -> NoReturn:
    raise Exception()

def f():
    always_raises()

    # TODO: we should mark this as unreachable
    print("unreachable")
```

### Python version and platform checks

It is common to have code that is specific to a certain Python version or platform. This case is
special because whether or not the code is reachable depends on externally configured constants. And
if we are checking for a set of parameters that makes one of these branches unreachable, that is
likely not something that the user wants to be warned about, because there are probably other sets
of parameters that make the branch reachable.

#### `sys.version_info` branches

Consider the following example. If we check with a Python version lower than 3.11, the import
statement is unreachable. If we check with a Python version equal to or greater than 3.11, the
import statement is definitely reachable. We should not emit any diagnostics in either case.

##### Checking with Python version 3.10

```toml
[environment]
python-version = "3.10"
```

```py
import sys

if sys.version_info >= (3, 11):
    from typing import Self
```

##### Checking with Python version 3.12

```toml
[environment]
python-version = "3.12"
```

```py
import sys

if sys.version_info >= (3, 11):
    from typing import Self
```

#### `sys.platform` branches

The problem is even more pronounced with `sys.platform` branches, since we don't necessarily have
the platform information available.

##### Checking with platform `win32`

```toml
[environment]
python-platform = "win32"
```

```py
import sys

if sys.platform == "win32":
    sys.getwindowsversion()
```

##### Checking with platform `linux`

```toml
[environment]
python-platform = "linux"
```

```py
import sys

if sys.platform == "win32":
    sys.getwindowsversion()
```

##### Checking with platform set to `all`

```toml
[environment]
python-platform = "all"
```

If `python-platform` is set to `all`, we treat the platform as unspecified. This means that we do
not infer a literal type like `Literal["win32"]` for `sys.platform`, but instead fall back to
`LiteralString` (the `typeshed` annotation for `sys.platform`). This means that we can not
statically determine the truthiness of a branch like `sys.platform == "win32"`.

See <https://github.com/astral-sh/ruff/issues/16983#issuecomment-2777146188> for a plan on how this
could be improved.

```py
import sys

if sys.platform == "win32":
    # TODO: we should not emit an error here
    # error: [possibly-unbound-attribute]
    sys.getwindowsversion()
```

## No (incorrect) diagnostics in unreachable code

```toml
[environment]
python-version = "3.10"
```

In this section, we demonstrate that we do not emit (incorrect) diagnostics in unreachable sections
of code.

It could be argued that no diagnostics at all should be emitted in unreachable code. The reasoning
is that any issues inside the unreachable section would not cause problems at runtime. And type
checking the unreachable code under the assumption that it *is* reachable might lead to false
positives (see the "Global constants" example below).

On the other hand, it could be argued that code like `1 + "a"` is incorrect, no matter if it is
reachable or not. Some developers like to use things like early `return` statements while debugging,
and for this use case, it is helpful to still see some diagnostics in unreachable sections.

We currently follow the second approach, but we do not attempt to provide the full set of
diagnostics in unreachable sections. In fact, we silence a certain category of diagnostics
(`unresolved-reference`, `unresolved-attribute`, …), in order to avoid *incorrect* diagnostics. In
the future, we may revisit this decision.

### Use of variables in unreachable code

We should not emit any diagnostics for uses of symbols in unreachable code:

```py
def f():
    x = 1
    return

    print("unreachable")

    print(x)
```

### Use of variable in nested function

In the example below, since we use `x` in the `inner` function, we use the "public" type of `x`,
which currently refers to the end-of-scope type of `x`. Since the end of the `outer` scope is
unreachable, we need to make sure that we do not emit an `unresolved-reference` diagnostic:

```py
def outer():
    x = 1

    def inner():
        reveal_type(x)  # revealed: Unknown
    while True:
        pass
```

### Global constants

```py
from typing import Literal

FEATURE_X_ACTIVATED: Literal[False] = False

if FEATURE_X_ACTIVATED:
    def feature_x():
        print("Performing 'X'")

def f():
    if FEATURE_X_ACTIVATED:
        # Type checking this particular section as if it were reachable would
        # lead to a false positive, so we should not emit diagnostics here.

        feature_x()
```

### Exhaustive check of syntactic constructs

We include some more examples here to make sure that silencing of diagnostics works for
syntactically different cases. To test this, we use `ExceptionGroup`, which is only available in
Python 3.11 and later. We have set the Python version to 3.10 for this whole section, to have
`match` statements available, but not `ExceptionGroup`.

To start, we make sure that we do not emit a diagnostic in this simple case:

```py
import sys

if sys.version_info >= (3, 11):
    ExceptionGroup  # no error here
```

Similarly, if we negate the logic, we also emit no error:

```py
if sys.version_info < (3, 11):
    pass
else:
    ExceptionGroup  # no error here
```

This also works for more complex `if`-`elif`-`else` chains:

```py
if sys.version_info >= (3, 13):
    ExceptionGroup  # no error here
elif sys.version_info >= (3, 12):
    ExceptionGroup  # no error here
elif sys.version_info >= (3, 11):
    ExceptionGroup  # no error here
elif sys.version_info >= (3, 10):
    pass
else:
    pass
```

And for nested `if` statements:

```py
def _(flag: bool):
    if flag:
        if sys.version_info >= (3, 11):
            ExceptionGroup  # no error here
        else:
            pass

        if sys.version_info < (3, 11):
            pass
        else:
            ExceptionGroup  # no error here
```

The same works for ternary expressions:

```py
class ExceptionGroupPolyfill: ...

MyExceptionGroup1 = ExceptionGroup if sys.version_info >= (3, 11) else ExceptionGroupPolyfill
MyExceptionGroup1 = ExceptionGroupPolyfill if sys.version_info < (3, 11) else ExceptionGroup
```

Due to short-circuiting, this also works for Boolean operators:

```py
sys.version_info >= (3, 11) and ExceptionGroup
sys.version_info < (3, 11) or ExceptionGroup
```

And in `match` statements:

```py
reveal_type(sys.version_info.minor)  # revealed: Literal[10]

match sys.version_info.minor:
    case 13:
        ExceptionGroup
    case 12:
        ExceptionGroup
    case 11:
        ExceptionGroup
    case _:
        pass
```

Terminal statements can also lead to unreachable code:

```py
def f():
    if sys.version_info < (3, 11):
        raise RuntimeError("this code only works for Python 3.11+")

    ExceptionGroup
```

Similarly, assertions with statically-known falsy conditions can lead to unreachable code:

```py
def f():
    assert sys.version_info > (3, 11)

    ExceptionGroup
```

Finally, not that anyone would ever use it, but it also works for `while` loops:

```py
while sys.version_info >= (3, 11):
    ExceptionGroup
```

### Silencing errors for actually unknown symbols

We currently also silence diagnostics for symbols that are not actually defined anywhere. It is
conceivable that this could be improved, but is not a priority for now.

```py
if False:
    does_not_exist

def f():
    return
    does_not_exist
```

### Attributes

When attribute expressions appear in unreachable code, we should not emit `unresolved-attribute`
diagnostics:

```py
import sys
import builtins

if sys.version_info >= (3, 11):
    builtins.ExceptionGroup
```

### Imports

When import statements appear in unreachable code, we should not emit `unresolved-import`
diagnostics:

```py
import sys

if sys.version_info >= (3, 11):
    from builtins import ExceptionGroup

    import builtins.ExceptionGroup

    # See https://docs.python.org/3/whatsnew/3.11.html#new-modules

    import tomllib

    import wsgiref.types
```

### Nested scopes

When we have nested scopes inside the unreachable section, we should not emit diagnostics either:

```py
if False:
    x = 1

    def f():
        print(x)

    class C:
        def __init__(self):
            print(x)
```

### Type annotations

Silencing of diagnostics also works for type annotations, even if they are stringified:

```py
import sys
import typing

if sys.version_info >= (3, 11):
    from typing import Self

    class C:
        def name_expr(self) -> Self:
            return self

        def name_expr_stringified(self) -> "Self":
            return self

        def attribute_expr(self) -> typing.Self:
            return self

        def attribute_expr_stringified(self) -> "typing.Self":
            return self
```

### Use of unreachable symbols in type annotations, or as class bases

We should not show any diagnostics in type annotations inside unreachable sections.

```py
def _():
    class C:
        class Inner: ...

    return

    c1: C = C()
    c2: C.Inner = C.Inner()
    c3: tuple[C, C] = (C(), C())
    c4: tuple[C.Inner, C.Inner] = (C.Inner(), C.Inner())

    class Sub(C): ...
```

### Emit diagnostics for definitely wrong code

Even though the expressions in the snippet below are unreachable, we still emit diagnostics for
them:

```py
if False:
    1 + "a"  # error: [unsupported-operator]

def f():
    return

    1 / 0  # error: [division-by-zero]
```

## Limitations of the current approach

The current approach of silencing only a subset of diagnostics in unreachable code leads to some
problems, and we may want to re-evaluate this decision in the future. To illustrate, consider the
following example:

```py
if False:
    x: int = 1
else:
    x: str = "a"

if False:
    # TODO We currently emit a false positive here:
    # error: [invalid-assignment] "Object of type `Literal["a"]` is not assignable to `int`"
    other: int = x
else:
    other: str = x
```

The problem here originates from the fact that the type of `x` in the `False` branch conflicts with
the visible type of `x` in the `True` branch. When we type-check the lower `False` branch, we only
see the visible definition of `x`, which has a type of `str`.

In principle, this means that all diagnostics that depend on type information from "outside" the
unreachable section should be silenced. Similar problems to the one above can occur for other rule
types as well:

```py
from typing import Literal

if False:
    def f(x: int): ...
    def g(*, a: int, b: int): ...

    class C:
        x: int = 1

    class D:
        def __call__(self):
            pass

    number: Literal[1] = 1
else:
    def f(x: str): ...
    def g(*, a: int): ...

    class C:
        x: str = "a"

    class D: ...
    number: Literal[0] = 0

if False:
    # TODO
    # error: [invalid-argument-type]
    f(2)

    # TODO
    # error: [unknown-argument]
    g(a=2, b=3)

    # TODO
    # error: [invalid-assignment]
    C.x = 2

    d: D = D()
    # TODO
    # error: [call-non-callable]
    d()

    # TODO
    # error: [division-by-zero]
    1 / number
```
