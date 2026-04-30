# `global` references

## Implicit global in function

A name reference to a never-defined symbol in a function is implicitly a global lookup.

```py
x = 1

def f():
    reveal_type(x)  # revealed: Literal[1]
```

## Explicit global in function

```py
x = 1

def f():
    global x
    reveal_type(x)  # revealed: Literal[1]
```

## Unassignable type in function

```py
x: int = 1

def f():
    y: int = 1
    # error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    y = ""

    global x
    # error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    x = ""

    global z
    # error: [invalid-assignment] "Object of type `Literal[""]` is not assignable to `int`"
    z = ""

z: int
```

## Nested intervening scope

A `global` statement causes lookup to skip any bindings in intervening scopes:

```py
x: int = 1

def outer():
    x: str = ""

    def inner():
        global x
        reveal_type(x)  # revealed: int
```

## Narrowing

An assignment following a `global` statement should narrow the type in the local scope after the
assignment.

```py
x: int | None

def f():
    global x
    x = 1
    reveal_type(x)  # revealed: Literal[1]
```

Same for an `if` statement:

```py
x: int | None

def f():
    # The `global` keyword isn't necessary here, but this is testing that it doesn't get in the way
    # of narrowing.
    global x
    if x == 1:
        y: int = x  # allowed, because x cannot be None in this branch
```

## Nested function after conditional rebinding

A nested function should resolve a `global` name through the enclosing scope, even if that scope
conditionally rebinds it. We conservatively include the binding before the early return, because we
do not try to prove that `inner` cannot run on that path:

```py
x = 1

def outer(flag: bool) -> None:
    global x

    if flag:
        x = 2
        return

    def inner() -> None:
        reveal_type(x)  # revealed: Literal[1, 2]
```

Without the early return, the nested function should see both possible bindings. This is a known
limitation: we currently infer only the rebound value instead of the union of both:

```py
x = 1

def outer(flag: bool) -> None:
    global x

    if flag:
        x = 2

    def inner() -> None:
        # TODO: should be `Literal[1, 2]`
        reveal_type(x)  # revealed: Literal[2]

    inner()
```

## `nonlocal` and `global`

A binding cannot be both `nonlocal` and `global`. This should emit a semantic syntax error. CPython
marks the `nonlocal` line, while `mypy`, `pyright`, and `ruff` (`PLE0115`) mark the `global` line.

```py
x = 1

def f():
    x = 1
    def g() -> None:
        nonlocal x
        global x  # error: [invalid-syntax] "name `x` is nonlocal and global"
        x = None
```

## Global declaration after `global` statement

```py
def f():
    global x
    y = x
    x = 1  # No error.

x = 2
```

## Semantic syntax errors

Using a name prior to its `global` declaration in the same scope is a syntax error.

```py
x = 1
y = 2

def f():
    print(x)
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    global x
    print(x)
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    print(x)
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    global x, y
    print(x)
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    print(x)

def f():
    x = 1
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    x = 1

def f():
    global x
    x = 1
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    x = 1

def f():
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x, y
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    del x
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x
    del x
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    global x, y
    del x
    global x, y  # error: [invalid-syntax] "name `x` is used prior to global declaration"
    del x

def f():
    print(f"{x=}")
    global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"

# still an error in module scope
x = None
global x  # error: [invalid-syntax] "name `x` is used prior to global declaration"
```

## Local `global` bindings dominate module-scope bindings

```py
x = 42

def f():
    global x
    reveal_type(x)  # revealed: Literal[42, "56"]
    x = "56"
    reveal_type(x)  # revealed: Literal["56"]
```

When a `global` augmented assignment has no module-scope binding to use as its base value, we
include an imprecise contribution rather than recursing through the same global symbol indefinitely:

```py
def f():
    global z  # error: [unresolved-global] "Invalid global declaration of `z`: `z` has no declarations or bindings in the global scope"
    z += 1

# error: [possibly-unresolved-reference] "Name `z` used when possibly not defined"
reveal_type(z)  # revealed: Unknown
```

## Imported globals include nested `global` bindings

`module.py`:

```py
x = 1

def f():
    global x
    x = "two"
```

`main.py`:

```py
from module import x

reveal_type(x)  # revealed: Literal[1, "two"]
```

## Local assignment prevents falling back to the outer scope

```py
x = 42

def f():
    # error: [unresolved-reference] "Name `x` used when not defined"
    reveal_type(x)  # revealed: Unknown
    x = "56"
    reveal_type(x)  # revealed: Literal["56"]
```

## Annotating a `global` binding is a syntax error

```py
x: int = 1

def f():
    global x
    x: str = "foo"  # error: [invalid-syntax] "annotated name `x` can't be global"
```

## Global declarations affect the inferred type of the binding

Even if the `global` declaration isn't used in an assignment, we conservatively assume it could be:

```py
x = 1

def f():
    global x

# TODO: reveal_type(x)  # revealed: Unknown | Literal["1"]
```

## Global variables need an explicit definition in the global scope

You're allowed to use the `global` keyword to define new global variables that don't have any
explicit definition in the global scope, but we consider that fishy and prefer to lint on it:

```py
x = 1
y: int
# z is neither bound nor declared in the global scope

def f():
    global x, y, z  # error: [unresolved-global] "Invalid global declaration of `z`: `z` has no declarations or bindings in the global scope"
```

You don't need a definition for implicit globals, but you do for built-ins:

```py
def f():
    global __file__  # allowed, implicit global
    global int  # error: [unresolved-global] "Invalid global declaration of `int`: `int` has no declarations or bindings in the global scope"
```

## Nested class after global rebinding

Even if a `global` declaration is unresolved at module scope, nested eager scopes in the same
function should still see a rebinding that already happened:

```py
def factory():
    global x  # error: [unresolved-global] "Invalid global declaration of `x`: `x` has no declarations or bindings in the global scope"
    x = 1

    class C:
        reveal_type(x)  # revealed: Literal[1]
```

## References to variables before they are defined within a class scope are considered global

If we try to access a variable in a class before it has been defined, the lookup will fall back to
global.

```py
import secrets

x: str = "a"

def f(x: int, y: int):
    class C:
        reveal_type(x)  # revealed: int

    class D:
        x = None
        reveal_type(x)  # revealed: None

    class E:
        reveal_type(x)  # revealed: str
        x = None

        # error: [unresolved-reference]
        reveal_type(y)  # revealed: Unknown
        y = None

    # Declarations count as definitions, even if there's no binding.
    class F:
        reveal_type(x)  # revealed: str
        x: int
        reveal_type(x)  # revealed: str

    # Explicitly `nonlocal` variables don't count, even if they're bound.
    class G:
        nonlocal x
        reveal_type(x)  # revealed: int
        x = 42
        reveal_type(x)  # revealed: Literal[42]

    # Possibly-unbound variables get unioned with the fallback lookup.
    class H:
        if secrets.randbelow(2):
            x = None
        reveal_type(x)  # revealed: None | str
```
