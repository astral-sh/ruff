# Return type inference

```toml
[environment]
python-version = "3.14"  # I like using modern syntax
```

## Basic

For a simple standalone function with a missing return type annotation, ty can infer the return type
based on the type of the expression in the `return` statement.

```py
def returns_1():
    return 1

def returns_a():
    return "a"

def returns_none():
    return None

reveal_type(returns_1())  # revealed: Literal[1]
reveal_type(returns_a())  # revealed: Literal["a"]
reveal_type(returns_none())  # revealed: None
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
# TODO: should be `def returns_1() -> Literal[1]`
reveal_type(returns_1)  # revealed: def returns_1() -> Unknown
# TODO: should be `def returns_a() -> Literal["a"]`
reveal_type(returns_a)  # revealed: def returns_a() -> Unknown
# TODO: should be `def returns_none() -> None`
reveal_type(returns_none)  # revealed: def returns_none() -> Unknown
reveal_type(returns_unknown)  # revealed: def returns_unknown(unknown) -> Unknown
```

If a function has multiple return statements with different types, ty unions those to determine the
overall return type:

```py
def returns_literal_1_or_a(flag: bool):
    if flag:
        return 1
    else:
        return "a"

reveal_type(returns_literal_1_or_a(True))  # revealed: Literal[1, "a"]
```

If control flow in a function can reach the end, or if all return statements `return` without an
expression, ty infers the return type as `None`:

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

reveal_type(implicit_none_1())  # revealed: None
reveal_type(implicit_none_2())  # revealed: None
reveal_type(implicit_none_3(True))  # revealed: None
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

reveal_type(returns_int_or_none_1(True))  # revealed: Literal[1] | None
reveal_type(returns_int_or_none_2(True))  # revealed: Literal[1] | None
```

Return type inference takes narrowed types into account, so this function can only return `str`:

```py
def to_str(x: int | str):
    if isinstance(x, int):
        return str(x)
    return x

reveal_type(to_str(1))  # revealed: str
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

reveal_type(returns_from_loop(True))  # revealed: Literal[1, "a"]
```

### Unreachable `return` statements

The `else` branch here is unreachable, so the inferred type should only consider the first branch.

```py
def can_only_return_int(flag: bool):
    if 1 + 1 == 2:
        return 1
    else:
        return "a"

reveal_type(can_only_return_int(True))  # revealed: Literal[1]
```

### Branches that do not return

Here, the `else` branch never returns, so there is no path that implicitly returns `None`:

```py
def non_returning_else(flag: bool):
    if flag:
        return 1
    else:
        raise Exception("This branch never returns")

reveal_type(non_returning_else(True))  # revealed: Literal[1]
```

### Exhaustive matches

```py
def type_name(x: int | str):
    match x:
        case int():
            return "int"
        case str():
            return "str"

reveal_type(type_name(1))  # revealed: Literal["int", "str"]
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
reveal_type(returns_from_finally())  # revealed: Literal[1, "a"]
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

reveal_type(returns_from_try_or_finally(True))  # revealed: Literal[1, "a"]
```

## Functions that do not return

### Infinite loops

A function with an infinite loop does not return, so its inferred return type should be `Never`.

```py
def infinite_loop():
    while True:
        pass

reveal_type(infinite_loop())  # revealed: Never
```

### Functions that always raise exceptions

Similarly, a function that always raises an exception, should also have an inferred return type of
`Never`.

```py
def always_raises():
    raise Exception("This function always raises")

reveal_type(always_raises())  # revealed: Never
```

### Functions that call other functions that do not return

A function that calls a function like `sys.exit()` never returns, so its inferred return type should
be `Never`.

```py
import sys

def my_exit():
    sys.exit()

reveal_type(my_exit())  # revealed: Never
```

## Generic functions

A generic function without a return type annotation can also have its return type inferred:

```py
def generic_identity[T](x: T):
    return x

reveal_type(generic_identity(1))  # revealed: Literal[1]
reveal_type(generic_identity("a"))  # revealed: Literal["a"]

def swap[X, Y](x: X, y: Y):
    return y, x

def _(x: int, y: str):
    reveal_type(swap(x, y))  # revealed: tuple[str, int]
```

If there are multiple paths, the inferred return type can be included in a union:

```py
def returns_t_or_none[T](x: T, flag: bool):
    if flag:
        return x

def _(flag: bool):
    reveal_type(returns_t_or_none(1, flag))  # revealed: Literal[1] | None
    reveal_type(returns_t_or_none("a", flag))  # revealed: Literal["a"] | None
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

Unfortunately, this means that we cannot infer precise types for attributes/methods on those types:

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

reveal_type(callable1)  # revealed: () -> int
reveal_type(callable1())  # revealed: int
```

### Nested definitions and generics

This case is a variant of the nested class problem, where an attribute depends on a function-scoped
type variable. This is probably not too important in practice, so it's probably fine if our behavior
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

## Generators

```py
def yields_and_returns():
    yield "yield"
    return "return"

# TODO: should ideally be `GeneratorType[Literal["yield"], Any, Literal["return"]]`
# once we also support yield-type inference. With return type inference alone, we
# should at least infer `GeneratorType[Any, Any, Literal["return"]]`.
reveal_type(yields_and_returns())  # revealed: Unknown
```

## Asynchronous functions

For async functions, the inferred return type will be wrapped in `CoroutineType`:

```py
async def async_func():
    return 1

# TODO: should be `def async_func() -> CoroutineType[Any, Any, Literal[1]]`
reveal_type(async_func)  # revealed: def async_func() -> CoroutineType[Any, Any, Unknown]
```

## Decorated functions

Decorators that pass through the original function should not affect return type inference:

```py
from typing import Callable

def identity_1[T](func: T) -> T:
    return func

@identity_1
def returns_1():
    return 1

reveal_type(returns_1())  # revealed: Literal[1]
```

This should also work with decorators that use `Callable` and `ParamSpec`:

```py
def identity_2[**P, R](func: Callable[P, R]) -> Callable[P, R]:
    return func

@identity_2
def returns_2():
    return 2

# TODO: should be `Literal[2]`
reveal_type(returns_2())  # revealed: Unknown
```

pyright can also handle unannotated decorators, but this might be much harder to support:

```py
def identity_3(func):
    return func

@identity_3  # error: [dynamic-function-decorator-return]
def returns_3():
    return 3

# TODO: should ideally be `Literal[3]`
reveal_type(returns_3())  # revealed: Unknown
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
promote to `int` though (instead of `Literal[1]`), or otherwise we would even get errors on an
override that returns `2`.

So here is the example without return type annotations:

```py
class BaseNA:
    def method(self):
        return 1

class DerivedNA(BaseNA):
    def method(self):
        return "a"

def _(base: BaseNA, derived: DerivedNA):
    reveal_type(base.method())  # revealed: Literal[1] | Unknown
    reveal_type(derived.method())  # revealed: Literal["a"] | Unknown
```

### Final classes and methods

A method on a final class, or a final method, cannot be overridden. So its inferred return type can
be precise and does not need to be widened:

```py
from typing import final

@final
class FinalClass:
    def method(self):
        return 1

class SomeClass:
    @final
    def final_method(self):
        return "a"

reveal_type(FinalClass().method())  # revealed: Literal[1]
reveal_type(SomeClass().final_method())  # revealed: Literal["a"]
```

### Overriding annotated base class methods

Another interesting case is when the base class method has an explicit return type annotation, but
the child class does not:

```py
class Parent:
    def method(self) -> int | None:
        return None

class Child(Parent):
    def method(self):
        return 1
```

In this case, we let the child class "inherit" the base class annotation:

```py
reveal_type(Child().method())  # revealed: int | None
```

This also works for subclasses further down:

```py
class GrandChild(Child):
    def method(self):
        return 2

reveal_type(GrandChild().method())  # revealed: int | None
```

When a subclass method returns an incompatible type compared to its base class annotation, we union
the inferred return type with the base class annotation:

```py
class IncompatibleChild(Parent):
    # TODO: This should be an invalid-method-override error.
    def method(self):
        return "a"

reveal_type(IncompatibleChild().method())  # revealed: Literal["a"] | int | None
```

Base methods are resolved in MRO order, including when an earlier base has no such method:

```py
class Empty: ...

class Other:
    def method(self) -> str:
        return "a"

class Multiple(Empty, Child, Other):
    def method(self):
        return 3

reveal_type(Multiple().method())  # revealed: int | None
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
    reveal_type(task.compute())  # revealed: None | Unknown
    reveal_type(job.run())  # revealed: Unknown
```

### Methods returning `self`

A method that returns `self` should have its return type inferred as `Self`, so that subclasses get
the return type specialized accordingly:

```py
class Fluent:
    def set_value(self, value: int):
        self.value = value
        return self

Fluent().set_value(1).set_value(2)

# TODO: Should be `Fluent`
reveal_type(Fluent().set_value(1))  # revealed: Fluent | Unknown

class FluentSub(Fluent): ...

FluentSub().set_value(1).set_value(2)

# TODO: Should be `FluentSub`
reveal_type(FluentSub().set_value(1))  # revealed: FluentSub | Unknown
```

This also works when the type of `self` has been widened or narrowed:

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

### Methods returning `cls`

Similar to the above, a classmethod that returns `cls` should have its return type inferred as
`type[Self]`:

```py
class Factory:
    @classmethod
    def reference(cls):
        return cls

    @classmethod
    def create(cls):
        return cls()

class FactorySub(Factory): ...

# TODO: Should be `type[Factory]`
reveal_type(Factory.reference())  # revealed: type[Factory] | Unknown

# TODO: Should be `Factory`
reveal_type(Factory.create())  # revealed: Factory | Unknown

# TODO: Should be `type[FactorySub]`
reveal_type(FactorySub.reference())  # revealed: type[FactorySub] | Unknown

# TODO: Should be `FactorySub`
reveal_type(FactorySub.create())  # revealed: FactorySub | Unknown
```

## Overloaded functions

Return type inference (on the actual implementation function) should not influence the return type
of the overload set somehow:

```py
from typing import overload

@overload
def func(x: int) -> int: ...
@overload
def func(x: str) -> str: ...
def func(x):
    # This wrong return type should not affect the overall return type
    # TODO: Ideally, this would be an error
    return b"wrong"

reveal_type(func(1))  # revealed: int
reveal_type(func("a"))  # revealed: str
```

## Stub files

Stub bodies do not provide return type information. An omitted annotation remains `Unknown`:

`library.pyi`:

```pyi
def value(): ...
```

`main.py`:

```py
from library import value

reveal_type(value())  # revealed: Unknown
```

## Recursive functions

### Basic

Recursive functions can make return type inference more interesting, since type inference of the
returned expression can be self-referential:

```py
def factorial(n: int):
    if n == 0:
        return 1
    return n * factorial(n - 1)

# TODO: Should ideally be `def factorial(n: int) -> int`
reveal_type(factorial)  # revealed: def factorial(n: int) -> Unknown
reveal_type(factorial(5))  # revealed: int

def fibonacci(n: int):
    if n == 0:
        return 0
    if n == 1:
        return 1
    return fibonacci(n - 1) + fibonacci(n - 2)

# TODO: Should ideally be `def fibonacci(n: int) -> int`
reveal_type(fibonacci)  # revealed: def fibonacci(n: int) -> Unknown
reveal_type(fibonacci(5))  # revealed: int
```

In some cases, fixed-point iteration might be able to determine a precise return type:

```py
def alternating(n: int):
    if n <= 0:
        return 1
    return -alternating(n - 1)

# TODO: Should ideally be `Literal[-1, 1]`, or at least `Literal[-1, 1] | Unknown`
reveal_type(alternating(3))  # revealed: int
```

### Divergent

A function that would never return normally should still be analyzed without errors:

```py
def divergent():
    return divergent()

# TODO: Should ideally be `Never`, but `Unknown` is also okay
reveal_type(divergent())  # revealed: Divergent

def expanding(x):
    return (expanding(x), expanding(x))

# TODO: Should ideally be `Never` or `tuple[Never, Never]`, but `Unknown` is also okay
reveal_type(expanding(5))  # revealed: tuple[Divergent, Divergent]
```

### Mutual recursion

Functions that mutually depend on each others' return values can also be analyzed:

```py
def left(n: int):
    if n <= 0:
        return 1
    return right(n - 1)

def right(n: int):
    if n <= 0:
        return "a"
    return left(n - 1)

reveal_type(left(3))  # revealed: Literal[1, "a"]
reveal_type(right(3))  # revealed: Literal["a", 1]
```
