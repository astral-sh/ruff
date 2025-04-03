# Unreachable code

## Detecting unreachable code

In this section, we look at various scenarios how sections of code can become unreachable. We should
eventually introduce a new diagnostic that would detect unreachable code.

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

## Python version and platform checks

It is common to have code that is specific to a certain Python version or platform. This case is
special because whether or not the code is reachable depends on externally configured constants. And
if we are checking for a set of parameters that makes one of these branches unreachable, that is
likely not something that the user wants to be warned about, because there are probably other sets
of parameters that make the branch reachable.

### `sys.version_info` branches

Consider the following example. If we check with a Python version lower than 3.11, the import
statement is unreachable. If we check with a Python version equal to or greater than 3.11, the
import statement is definitely reachable. We should not emit any diagnostics in either case.

#### Checking with Python version 3.10

```toml
[environment]
python-version = "3.10"
```

```py
import sys

if sys.version_info >= (3, 11):
    # TODO: we should not emit an error here
    # error: [unresolved-import]
    from typing import Self
```

#### Checking with Python version 3.12

```toml
[environment]
python-version = "3.12"
```

```py
import sys

if sys.version_info >= (3, 11):
    from typing import Self
```

### `sys.platform` branches

The problem is even more pronounced with `sys.platform` branches, since we don't necessarily have
the platform information available.

#### Checking with platform `win32`

```toml
[environment]
python-platform = "win32"
```

```py
import sys

if sys.platform == "win32":
    sys.getwindowsversion()
```

#### Checking with platform `linux`

```toml
[environment]
python-platform = "linux"
```

```py
import sys

if sys.platform == "win32":
    # TODO: we should not emit an error here
    # error: [unresolved-attribute]
    sys.getwindowsversion()
```

#### Checking without a specified platform

```toml
[environment]
# python-platform not specified
```

```py
import sys

if sys.platform == "win32":
    # TODO: we should not emit an error here
    # error: [possibly-unbound-attribute]
    sys.getwindowsversion()
```

#### Checking with platform set to `all`

```toml
[environment]
python-platform = "all"
```

```py
import sys

if sys.platform == "win32":
    # TODO: we should not emit an error here
    # error: [possibly-unbound-attribute]
    sys.getwindowsversion()
```

## No false positive diagnostics in unreachable code

In this section, we make sure that we do not emit false positive diagnostics in unreachable code.

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
unreachable, we treat `x` as if it was not defined. This behavior can certainly be improved.

```py
def outer():
    x = 1

    def inner():
        return x  # Name `x` used when not defined
    while True:
        pass
```

## No diagnostics in unreachable code

In general, no diagnostics should be emitted in unreachable code. The reasoning is that any issues
inside the unreachable section would not cause problems at runtime. And type checking the
unreachable code under the assumption that it *is* reachable might lead to false positives:

```py
FEATURE_X_ACTIVATED = False

if FEATURE_X_ACTIVATED:
    def feature_x():
        print("Performing 'X'")

def f():
    if FEATURE_X_ACTIVATED:
        # Type checking this particular section as if it were reachable would
        # lead to a false positive, so we should not emit diagnostics here.

        # TODO: no error should be emitted here
        # error: [unresolved-reference]
        feature_x()
```
