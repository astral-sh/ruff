# Invalid argument type diagnostics

<!-- snapshot-diagnostics -->

## Basic

This is a basic test demonstrating that a diagnostic points to the function definition corresponding
to the invalid argument.

```py
def foo(x: int) -> int:
    return x * x

foo("hello")  # error: [invalid-argument-type]
```

## Different source order

This is like the basic test, except we put the call site above the function definition.

```py
def bar():
    foo("hello")  # error: [invalid-argument-type]

def foo(x: int) -> int:
    return x * x
```

## Different files

This tests that a diagnostic can point to a function definition in a different file in which an
invalid call site was found.

`package.py`:

```py
def foo(x: int) -> int:
    return x * x
```

```py
import package

package.foo("hello")  # error: [invalid-argument-type]
```

## Many parameters

This checks that a diagnostic renders reasonably when there are multiple parameters.

```py
def foo(x: int, y: int, z: int) -> int:
    return x * y * z

foo(1, "hello", 3)  # error: [invalid-argument-type]
```

## Many parameters across multiple lines

This checks that a diagnostic renders reasonably when there are multiple parameters spread out
across multiple lines.

```py
def foo(
    x: int,
    y: int,
    z: int,
) -> int:
    return x * y * z

foo(1, "hello", 3)  # error: [invalid-argument-type]
```

## Many parameters with multiple invalid arguments

This checks that a diagnostic renders reasonably when there are multiple parameters and multiple
invalid argument types.

```py
def foo(x: int, y: int, z: int) -> int:
    return x * y * z

# error: [invalid-argument-type]
# error: [invalid-argument-type]
# error: [invalid-argument-type]
foo("a", "b", "c")
```

At present (2025-02-18), this renders three different diagnostic messages. But arguably, these could
all be folded into one diagnostic. Fixing this requires at least better support for multi-spans in
the diagnostic model and possibly also how diagnostics are emitted by the type checker itself.

## Test calling a function whose type is vendored from `typeshed`

This tests that diagnostic rendering is reasonable when the function being called is from the
standard library.

```py
import json

json.loads(5)  # error: [invalid-argument-type]
```

## Tests for a variety of argument types

These tests check that diagnostic output is reasonable regardless of the kinds of arguments used in
a function definition.

### Only positional

Tests a function definition with only positional parameters.

```py
def foo(x: int, y: int, z: int, /) -> int:
    return x * y * z

foo(1, "hello", 3)  # error: [invalid-argument-type]
```

### Variadic arguments

Tests a function definition with variadic arguments.

```py
def foo(*numbers: int) -> int:
    return len(numbers)

foo(1, 2, 3, "hello", 5)  # error: [invalid-argument-type]
```

### Keyword only arguments

Tests a function definition with keyword-only arguments.

```py
def foo(x: int, y: int, *, z: int = 0) -> int:
    return x * y * z

foo(1, 2, z="hello")  # error: [invalid-argument-type]
```

### One keyword argument

Tests a function definition with keyword-only arguments.

```py
def foo(x: int, y: int, z: int = 0) -> int:
    return x * y * z

foo(1, 2, "hello")  # error: [invalid-argument-type]
```

### Variadic keyword arguments

```py
def foo(**numbers: int) -> int:
    return len(numbers)

foo(a=1, b=2, c=3, d="hello", e=5)  # error: [invalid-argument-type]
```

### Mix of arguments

Tests a function definition with multiple different kinds of arguments.

```py
def foo(x: int, /, y: int, *, z: int = 0) -> int:
    return x * y * z

foo(1, 2, z="hello")  # error: [invalid-argument-type]
```

### Synthetic arguments

Tests a function call with synthetic arguments.

```py
class C:
    def __call__(self, x: int) -> int:
        return 1

c = C()
c("wrong")  # error: [invalid-argument-type]
```

## Calls to methods

Tests that we also see a reference to a function if the callable is a bound method.

```py
class C:
    def square(self, x: int) -> int:
        return x * x

c = C()
c.square("hello")  # error: [invalid-argument-type]
```
