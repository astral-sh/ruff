# Invalid argument type diagnostics

## Basic

This is a basic test demonstrating that a diagnostic points to the function definition corresponding
to the invalid argument.

```py
def foo(x: int) -> int:
    return x * x

foo("hello")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:4:5
  |
4 | foo("hello")  # snapshot: invalid-argument-type
  |     ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int) -> int:
  |     ^^^ ------ Parameter declared here
  |
```

## Different source order

This is like the basic test, except we put the call site above the function definition.

```py
def bar():
    foo("hello")  # snapshot: invalid-argument-type

def foo(x: int) -> int:
    return x * x
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:2:9
  |
2 |     foo("hello")  # snapshot: invalid-argument-type
  |         ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:4:5
  |
4 | def foo(x: int) -> int:
  |     ^^^ ------ Parameter declared here
  |
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

package.foo("hello")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:3:13
  |
3 | package.foo("hello")  # snapshot: invalid-argument-type
  |             ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/package.py:1:5
  |
1 | def foo(x: int) -> int:
  |     ^^^ ------ Parameter declared here
  |
```

## Many parameters

This checks that a diagnostic renders reasonably when there are multiple parameters.

```py
def foo(x: int, y: int, z: int) -> int:
    return x * y * z

foo(1, "hello", 3)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:4:8
  |
4 | foo(1, "hello", 3)  # snapshot: invalid-argument-type
  |        ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int, y: int, z: int) -> int:
  |     ^^^         ------ Parameter declared here
  |
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

foo(1, "hello", 3)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:8:8
  |
8 | foo(1, "hello", 3)  # snapshot: invalid-argument-type
  |        ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(
  |     ^^^
  |
 ::: src/mdtest_snippet.py:3:5
  |
3 |     y: int,
  |     ------ Parameter declared here
  |
```

## Many parameters with multiple invalid arguments

This checks that a diagnostic renders reasonably when there are multiple parameters and multiple
invalid argument types.

```py
def foo(x: int, y: int, z: int) -> int:
    return x * y * z

# snapshot: invalid-argument-type
# snapshot: invalid-argument-type
# snapshot: invalid-argument-type
foo("a", "b", "c")
```

At present (2025-02-18), this renders three different diagnostic messages. But arguably, these could
all be folded into one diagnostic. Fixing this requires at least better support for multi-spans in
the diagnostic model and possibly also how diagnostics are emitted by the type checker itself:

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:7:5
  |
7 | foo("a", "b", "c")
  |     ^^^ Expected `int`, found `Literal["a"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int, y: int, z: int) -> int:
  |     ^^^ ------ Parameter declared here
  |


error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:7:10
  |
7 | foo("a", "b", "c")
  |          ^^^ Expected `int`, found `Literal["b"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int, y: int, z: int) -> int:
  |     ^^^         ------ Parameter declared here
  |


error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:7:15
  |
7 | foo("a", "b", "c")
  |               ^^^ Expected `int`, found `Literal["c"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int, y: int, z: int) -> int:
  |     ^^^                 ------ Parameter declared here
  |
```

## Test calling a function whose type is vendored from `typeshed`

This tests that diagnostic rendering is reasonable when the function being called is from the
standard library.

```py
import json

json.loads(5)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `loads` is incorrect
 --> src/mdtest_snippet.py:3:12
  |
3 | json.loads(5)  # snapshot: invalid-argument-type
  |            ^ Expected `str | bytes | bytearray`, found `Literal[5]`
  |
info: Function defined here
   --> stdlib/json/__init__.pyi:224:5
    |
224 | def loads(
    |     ^^^^^
225 |     s: str | bytes | bytearray,
    |     -------------------------- Parameter declared here
    |
```

## Tests for a variety of argument types

These tests check that diagnostic output is reasonable regardless of the kinds of arguments used in
a function definition.

### Only positional

Tests a function definition with only positional parameters.

```py
def foo(x: int, y: int, z: int, /) -> int:
    return x * y * z

foo(1, "hello", 3)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:4:8
  |
4 | foo(1, "hello", 3)  # snapshot: invalid-argument-type
  |        ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int, y: int, z: int, /) -> int:
  |     ^^^         ------ Parameter declared here
  |
```

### Variadic arguments

Tests a function definition with variadic arguments.

```py
def foo(*numbers: int) -> int:
    return len(numbers)

foo(1, 2, 3, "hello", 5)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:4:14
  |
4 | foo(1, 2, 3, "hello", 5)  # snapshot: invalid-argument-type
  |              ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(*numbers: int) -> int:
  |     ^^^ ------------- Parameter declared here
  |
```

### Keyword only arguments

Tests a function definition with keyword-only arguments.

```py
def foo(x: int, y: int, *, z: int = 0) -> int:
    return x * y * z

foo(1, 2, z="hello")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:4:11
  |
4 | foo(1, 2, z="hello")  # snapshot: invalid-argument-type
  |           ^^^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int, y: int, *, z: int = 0) -> int:
  |     ^^^                    ---------- Parameter declared here
  |
```

### One keyword argument

Tests a function definition with keyword-only arguments.

```py
def foo(x: int, y: int, z: int = 0) -> int:
    return x * y * z

foo(1, 2, "hello")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:4:11
  |
4 | foo(1, 2, "hello")  # snapshot: invalid-argument-type
  |           ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int, y: int, z: int = 0) -> int:
  |     ^^^                 ---------- Parameter declared here
  |
```

### Variadic keyword arguments

```py
def foo(**numbers: int) -> int:
    return len(numbers)

foo(a=1, b=2, c=3, d="hello", e=5)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:4:20
  |
4 | foo(a=1, b=2, c=3, d="hello", e=5)  # snapshot: invalid-argument-type
  |                    ^^^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(**numbers: int) -> int:
  |     ^^^ -------------- Parameter declared here
  |
```

### Mix of arguments

Tests a function definition with multiple different kinds of arguments.

```py
def foo(x: int, /, y: int, *, z: int = 0) -> int:
    return x * y * z

foo(1, 2, z="hello")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `foo` is incorrect
 --> src/mdtest_snippet.py:4:11
  |
4 | foo(1, 2, z="hello")  # snapshot: invalid-argument-type
  |           ^^^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def foo(x: int, /, y: int, *, z: int = 0) -> int:
  |     ^^^                       ---------- Parameter declared here
  |
```

### Synthetic arguments

Tests a function call with synthetic arguments.

```py
class C:
    def __call__(self, x: int) -> int:
        return 1

c = C()
c("wrong")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to bound method `C.__call__` is incorrect
 --> src/mdtest_snippet.py:6:3
  |
6 | c("wrong")  # snapshot: invalid-argument-type
  |   ^^^^^^^ Expected `int`, found `Literal["wrong"]`
  |
info: Method defined here
 --> src/mdtest_snippet.py:2:9
  |
2 |     def __call__(self, x: int) -> int:
  |         ^^^^^^^^       ------ Parameter declared here
  |
```

## Calls to methods

Tests that we also see a reference to a function if the callable is a bound method.

```py
class C:
    def square(self, x: int) -> int:
        return x * x

c = C()
c.square("hello")  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to bound method `C.square` is incorrect
 --> src/mdtest_snippet.py:6:10
  |
6 | c.square("hello")  # snapshot: invalid-argument-type
  |          ^^^^^^^ Expected `int`, found `Literal["hello"]`
  |
info: Method defined here
 --> src/mdtest_snippet.py:2:9
  |
2 |     def square(self, x: int) -> int:
  |         ^^^^^^       ------ Parameter declared here
  |
```

## Types with the same name but from different files

`module.py`:

```py
class Foo: ...

def needs_a_foo(x: Foo): ...
```

`main.py`:

```py
from module import needs_a_foo

class Foo: ...

needs_a_foo(Foo())  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `needs_a_foo` is incorrect
 --> src/main.py:5:13
  |
5 | needs_a_foo(Foo())  # snapshot: invalid-argument-type
  |             ^^^^^ Expected `module.Foo`, found `main.Foo`
  |
info: Function defined here
 --> src/module.py:3:5
  |
3 | def needs_a_foo(x: Foo): ...
  |     ^^^^^^^^^^^ ------ Parameter declared here
  |
```

## TypeVars with bounds that have the same name but are from different files

In this case, using fully qualified names is *not* necessary.

```toml
[environment]
python-version = "3.12"
```

`module.py`:

```py
class Foo: ...

def needs_a_foo(x: Foo): ...
```

`main.py`:

```py
from module import needs_a_foo

class Foo: ...

def f[T: Foo](x: T) -> T:
    needs_a_foo(x)  # snapshot: invalid-argument-type
    return x
```

```snapshot
error[invalid-argument-type]: Argument to function `needs_a_foo` is incorrect
 --> src/main.py:6:17
  |
6 |     needs_a_foo(x)  # snapshot: invalid-argument-type
  |                 ^ Expected `Foo`, found `T@f`
  |
info: Function defined here
 --> src/module.py:3:5
  |
3 | def needs_a_foo(x: Foo): ...
  |     ^^^^^^^^^^^ ------ Parameter declared here
  |
```

## Numbers special case

```py
from numbers import Number

def f(x: Number): ...

f(5)  # snapshot: invalid-argument-type

def g(x: float):
    f(x)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `f` is incorrect
 --> src/mdtest_snippet.py:5:3
  |
5 | f(5)  # snapshot: invalid-argument-type
  |   ^ Expected `Number`, found `Literal[5]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:3:5
  |
3 | def f(x: Number): ...
  |     ^ --------- Parameter declared here
  |
info: Types from the `numbers` module aren't supported for static type checking
help: Consider using a protocol instead, such as `typing.SupportsFloat`


error[invalid-argument-type]: Argument to function `f` is incorrect
 --> src/mdtest_snippet.py:8:7
  |
8 |     f(x)  # snapshot: invalid-argument-type
  |       ^ Expected `Number`, found `int | float`
  |
info: element `int` of union `int | float` is not assignable to `Number`
info: Function defined here
 --> src/mdtest_snippet.py:3:5
  |
3 | def f(x: Number): ...
  |     ^ --------- Parameter declared here
  |
info: Types from the `numbers` module aren't supported for static type checking
help: Consider using a protocol instead, such as `typing.SupportsFloat`
```

## Invariant generic classes

We show a special diagnostic hint for invariant generic classes. For more details, see the
[`invalid_assignment_details.md`](./invalid_assignment_details.md) test.

```py
def modify(xs: list[int]):
    xs.append(42)

xs: list[bool] = [True, False]
modify(xs)  # snapshot: invalid-argument-type
```

```snapshot
error[invalid-argument-type]: Argument to function `modify` is incorrect
 --> src/mdtest_snippet.py:5:8
  |
5 | modify(xs)  # snapshot: invalid-argument-type
  |        ^^ Expected `list[int]`, found `list[bool]`
  |
info: Function defined here
 --> src/mdtest_snippet.py:1:5
  |
1 | def modify(xs: list[int]):
  |     ^^^^^^ ------------- Parameter declared here
  |
info: `list` is invariant in its type parameter
info: Consider using the covariant supertype `collections.abc.Sequence`
info: For more information, see https://docs.astral.sh/ty/reference/typing-faq/#invariant-generics
```
