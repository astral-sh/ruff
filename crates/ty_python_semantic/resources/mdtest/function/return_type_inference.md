# Return type inference

```toml
[environment]
python-version = "3.14"  # I like using modern syntax
```

## Basic

For a simple standalone function with a missing return type annotation, ty can infer the return type
based on the type of the expression in the `return` statement.

```py
def returns_int():
    return 1

def returns_str():
    return "a"

def returns_none():
    return None

# TODO: should be `Literal[1]`
reveal_type(returns_int())  # revealed: Unknown
# TODO: should be `Literal["a"]`
reveal_type(returns_str())  # revealed: Unknown
# TODO: should be `None`
reveal_type(returns_none())  # revealed: Unknown
```

If a return expression refers back to a parameter of unknown type, the inferred return type is
currently also inferred as `Unknown`.

pyright does something interesting for cases like these. It re-infers the body of "short" functions
at each call site, specializing the parameter type to the type of the argument passed at that call
site. This way, it is able to infer `Literal[1]` for `returns_unknown(1)` below.

```py
def returns_unknown(unknown):
    return unknown

reveal_type(returns_unknown(1))  # revealed: Unknown
reveal_type(returns_unknown(""))  # revealed: Unknown
```

For all the functions defined above, the inferred return type is also reflected in the inferred
signature:

```py
# TODO: should be `def returns_int() -> int`
reveal_type(returns_int)  # revealed: def returns_int() -> Unknown
# TODO: should be `def returns_str() -> str`
reveal_type(returns_str)  # revealed: def returns_str() -> Unknown
# TODO: should be `def returns_none() -> None`
reveal_type(returns_none)  # revealed: def returns_none() -> Unknown
reveal_type(returns_unknown)  # revealed: def returns_unknown(unknown) -> Unknown
```

If a function has multiple return statements with different types, ty unions those to determine the
overall return type:

```py
def returns_int_or_str(flag: bool):
    if flag:
        return 1
    else:
        return "a"

# TODO: should be `Literal[1, "a"]`
reveal_type(returns_int_or_str(True))  # revealed: Unknown
```

If the function has no return statements, or all return statements `return` without an expression,
ty infers the return type as `None`:

```py
def implicit_none_1():
    pass

def implicit_none_2():
    return

def implicit_none_3(flag: bool):
    if flag:
        return
    else:
        return

# TODO: should be `None`
reveal_type(implicit_none_1())  # revealed: Unknown
reveal_type(implicit_none_2())  # revealed: Unknown
reveal_type(implicit_none_3(True))  # revealed: Unknown
```

If the function has a mix of return statements with and without expressions, or some code paths that
do not have a return statement at all, ty will include `None` in the inferred return type to account
for the paths that return nothing:

```py
def returns_int_or_none_1(flag: bool):
    if flag:
        return 1

def returns_int_or_none_2(flag: bool):
    if flag:
        return 1
    else:
        return

# TODO: should be `Literal[1] | None`
reveal_type(returns_int_or_none_1(True))  # revealed: Unknown
reveal_type(returns_int_or_none_2(True))  # revealed: Unknown
```

Return type inference takes narrowed types into account, so this function can only return `str`:

```py
def to_str(x: int | str):
    if isinstance(x, int):
        return str(x)
    return x

# TODO: should be `str`
reveal_type(to_str(1))  # revealed: Unknown
```

## Functions with non-trivial control flow

### Returning from a loop

A `return` statement in a loop can be reached through multiple control flow paths, so the inferred
return type should be a union of all possible types:

```py
def returns_from_loop(some_condition):
    x = 1
    while True:
        if some_condition:
            return x
        x = "a"

# TODO: should be `Literal[1, "a"]`
reveal_type(returns_from_loop(True))  # revealed: Unknown
```

### Unreachable branches

The `else` branch here is unreachable, so the inferred type should only consider the first branch.

```py
def can_only_return_int(flag: bool):
    if 1 + 1 == 2:
        return 1
    else:
        return "a"

# TODO: should be `int`
reveal_type(can_only_return_int(True))  # revealed: Unknown
```

### Non-returning branches

Here, the `else` branch never returns, so there is no path that implicitly returns `None`:

```py
def non_returning_else(flag: bool):
    if flag:
        return 1
    else:
        raise Exception("This branch never returns")

# TODO: should be `Literal[1]`
reveal_type(non_returning_else(True))  # revealed: Unknown
```

### Returning from `finally` clauses

The `finally` clause always executes, even if there is a `return` statement in the `try` block. So
this function can only return `str`:

```py
def returns_from_finally():
    try:
        return 1
    finally:
        return "a"

# TODO: should ideally be `Literal["a"]`
reveal_type(returns_from_finally())  # revealed: Unknown
```

If the `return` in the `finally` clause is conditional, both the `try` block and the `finally` block
can contribute to the inferred return type.

```py
def returns_from_try_or_finally(flag: bool):
    try:
        return 1
    finally:
        if flag:
            return "a"

# TODO: should be `Literal[1] | Literal["a"]`
reveal_type(returns_from_try_or_finally(True))  # revealed: Unknown
```

## Functions that do not return

### Infinite loops

A function with an infinite loop does not return, so its inferred return type should be `Never`.

```py
def infinite_loop():
    while True:
        pass

# TODO: should be `Never`
reveal_type(infinite_loop())  # revealed: Unknown
```

### Functions that always raise exceptions

Similarly, a function that always raises an exception, should also have an inferred return type of
`Never`.

```py
def always_raises():
    raise Exception("This function always raises")

# TODO: should be `Never`
reveal_type(always_raises())  # revealed: Unknown
```

### Functions that call other functions that do not return

A function that calls a function like `sys.exit()` does never return, so its inferred return type
should be `Never`.

```py
import sys

def my_exit():
    sys.exit()

# TODO: should be `Never`
reveal_type(my_exit())  # revealed: Unknown
```

## Generic functions

A generic function without a return type annotation can also have its return type inferred:

```py
def generic_identity[T](x: T):
    return x

# TODO: should be `Literal[1]` or `int`
reveal_type(generic_identity(1))  # revealed: Unknown
# TODO: should be `Literal["a"]` or `str`
reveal_type(generic_identity("a"))  # revealed: Unknown

def swap[X, Y](x: X, y: Y):
    return y, x

def _(x: int, y: str):
    # TODO: should be `tuple[str, int]`
    reveal_type(swap(x, y))  # revealed: Unknown
```

If there are multiple paths, the inferred return type can be included in a union:

```py
def returns_t_or_none[T](x: T, flag: bool):
    if flag:
        return x

def _(flag: bool):
    # TODO: should be `Literal[1] | None` or `int | None`
    reveal_type(returns_t_or_none(1, flag))  # revealed: Unknown
    # TODO: should be `Literal["a"] | None` or `str | None`
    reveal_type(returns_t_or_none("a", flag))  # revealed: Unknown
```

## Internal structure that should not leak out

### Nested classes

We generally infer precise singleton types (`<class 'Inner'>`) for classes. However, if a nested
class is returned from a function, each function call would create a new class object, and they
wouldn't be identical:

```py
def returns_class():
    class Inner:
        attr: int

    reveal_type(Inner)  # revealed: <class 'Inner'>

    return Inner

Inner1 = returns_class()
Inner2 = returns_class()

# This should be `bool` or `Literal[False]`, not `Literal[True]`
reveal_type(Inner1 is Inner2)  # revealed: bool

# TODO: This should ideally be an error
i: Inner1 = Inner2()
```

Therefore, the return type of `returns_class` cannot be `<class 'Inner'>`. Instead, we currently
just treat it as `Unknown`.

```py
reveal_type(Inner1)  # revealed: Unknown
```

Unfortunatly, this means that we cannot infer precise types for attributes/methods on those types:

```py
reveal_type(Inner1().attr)  # revealed: Unknown
```

If we wanted to improve this, we could return a synthetic protocol representing the structure of the
nested class.

### Nested functions

The same problem occurs for nested functions. Here, we can promote the function to a corresponding
`Callable` type (which is basically what the synthetic protocol approach would do for nested
classes):

```py
def returns_callable():
    def inner() -> int:
        return 1

    reveal_type(inner)  # revealed: def inner() -> int

    return inner

callable1 = returns_callable()
callable2 = returns_callable()

# This should be `bool` or `Literal[False]`, not `Literal[True]`
reveal_type(callable1 is callable2)  # revealed: bool

# TODO: should be `Callable[[], int]`
reveal_type(callable1)  # revealed: Unknown
# TODO: should be `int`
reveal_type(callable1())  # revealed: Unknown
```

### Nested definitions and generics

This case is a variant of the nested class problem, where an attribute depends on a function-scoped
type variable. This is probably not too important in practise, so it's probably fine if our behavior
here is not perfect from the start.

```py
def returns_instance[T](x: T):
    class Inner:
        attr: T

        def __init__(self, attr: T) -> None:
            self.attr = attr

    return Inner(x)

def _(x: int):
    inner = returns_instance(x)

    # TODO: `Inner` would probably be wrong here. Ideally, we would return a synthetic protocol
    # with a type of `int` for the `attr` attribute.
    reveal_type(inner)  # revealed: Unknown

    # TODO: should ideally be `int`
    reveal_type(inner.attr)  # revealed: Unknown
```

## Asynchronous functions

For async functions, the inferred return type will be wrapped in `CoroutineType`:

```py
async def async_func():
    return 1

# TODO: should be `def async_func() -> CoroutineType[Any, Any, Literal[1]]`
reveal_type(async_func)  # revealed: def async_func() -> CoroutineType[Any, Any, Unknown]
```

## Methods

### Overrides

Methods on classes need special treatment for return type inference, since they can be overridden on
a subclass. To illustrate, consider the following example:

```py
class Base:
    def method(self) -> int | str:
        return 1

class Derived(Base):
    def method(self) -> int | str:
        return "a"
```

This is perfectly valid code. But now consider what happens if we remove the return type
annotations. If we were to infer `Literal[1]` or `int` as the return type of `Base.method`, just
like we do for free functions, we would consider `Derived.method` to be an invalid override, since
it returns a `str`. To maintain the gradual guarantee, we would need to infer the return type of
`Base.method` as `Literal[1] | Unknown`. This might be fine in practice, but if users don't like
seeing those unions with `Unknown`, we might also consider just inferring `int` as the return type
of `Base.method`, accepting the fact that it will lead to some false positives. We should definitely
promote to `int` though, or otherwise we would even get errors on an override that returns `2`.

So here is the example without return type annotations:

```py
class BaseNA:
    def method(self):
        return 1

class DerivedNA(BaseNA):
    def method(self):
        return "a"

def _(base: BaseNA, derived: DerivedNA):
    # TODO: Should either be `int` or `Literal[1] | Unknown`
    reveal_type(base.method())  # revealed: Unknown
    # TODO: Should either be `str` or `Literal["a"] | Unknown`
    reveal_type(derived.method())  # revealed: Unknown
```

### Methods that return `None` or `Never`

If a method returns `None` (explicitly or implicitly), or always raises, it seems particularly
important to widen the return type to `None | Unknown` and `Unknown`, respectively, since those
might just be base class implementations which are meant to be specialized in subclasses.

```py
class Task:
    def compute(self):
        return None

class IntTask(Task):
    def compute(self):
        return 1

class Job:
    def run(self):
        raise NotImplementedError

class IntJob(Job):
    def run(self):
        return 1

def _(task: Task, job: Job):
    # TODO: Should be `None | Unknown`
    reveal_type(task.compute())  # revealed: Unknown
    reveal_type(job.run())  # revealed: Unknown
```

### Methods returning `self`

A method that returns `self` should have its return type inferred as `Self`, so that sublasses get
the return type specialized accordingly:

```py
class Fluent:
    def set_value(self, value: int):
        self.value = value
        return self

Fluent().set_value(1).set_value(2)

# TODO: Should be `Fluent`
reveal_type(Fluent().set_value(1))  # revealed: Unknown

class FluentSub(Fluent): ...

FluentSub().set_value(1).set_value(2)

# TODO: Should be `FluentSub`
reveal_type(FluentSub().set_value(1))  # revealed: Unknown
```

This also works the type of `self` has widened or narrowed:

```py
class WidenedReceiver:
    def method(self: object):
        return self

class NarrowedReceiver:
    def method(self: NarrowedReceiverSub):
        return self

class NarrowedReceiverSub(NarrowedReceiver): ...

# TODO: Should be `def method(self: object) -> object`
reveal_type(WidenedReceiver.method)  # revealed: def method(self: object) -> Unknown

# TODO: Should be `def method(self: NarrowedReceiverSub) -> NarrowedReceiverSub`
reveal_type(NarrowedReceiver.method)  # revealed: def method(self: NarrowedReceiverSub) -> Unknown
```

## Recursive functions

To do
