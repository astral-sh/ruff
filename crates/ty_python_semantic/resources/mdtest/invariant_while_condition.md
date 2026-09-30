# Invariant while conditions

## Unchanged boolean

A local boolean that is never reassigned cannot change the outcome of a loop's condition.

```py
import time

def wait(condition: bool):
    while condition:  # snapshot: invariant-while-condition
        time.sleep(0.5)
        print("waiting...")
```

```snapshot
warning[invariant-while-condition]: Loop condition does not change between iterations
 --> src/mdtest_snippet.py:4:11
  |
4 |     while condition:  # snapshot: invariant-while-condition
  |           ^^^^^^^^^
help: If this is intentional, move the condition to an `if` statement and use `while True`
```

An explicit infinite loop inside an `if` makes this behavior intentional.

```py
def wait(condition: bool):
    if condition:
        while True:  # no diagnostic
            time.sleep(0.5)
```

## Compound conditions

Negation, boolean operators, and comparisons of immutable values can also remain unchanged.

```py
from typing import Literal

def negated(condition: bool):
    while not condition:  # error: [invariant-while-condition]
        print("waiting")

def compound(first: bool, second: bool):
    while first and second:  # error: [invariant-while-condition]
        print("waiting")

def changing_operand(first: bool, second: bool):
    while first and second:  # no diagnostic
        second = bool(input())

def comparison(first: bool, second: bool):
    while first != second:  # error: [invariant-while-condition]
        print("waiting")

def literals(value: Literal[0, 1, 2]):
    while value < 2:  # error: [invariant-while-condition]
        print("waiting")

def chained(value: Literal[0, 1, 2]):
    while 0 < value < 2:  # error: [invariant-while-condition]
        print("waiting")

def identity(value: object | None):
    while value is not None:  # error: [invariant-while-condition]
        print("waiting")
```

## Assignments and deletions

Rebinding the condition's local variables can change its truthiness. This includes assignments in
nested loops, exception handlers, and comprehensions.

```py
def reassigned(condition: bool):
    while condition:  # no diagnostic
        condition = bool(input())

def augmented(condition: bool):
    while condition:  # no diagnostic
        condition &= bool(input())

def deleted(condition: bool):
    while condition:  # error: [possibly-unresolved-reference]
        del condition  # error: [possibly-unresolved-reference]

def nested(condition: bool, values: list[bool]):
    while condition:  # no diagnostic
        for condition in values:
            pass

def comprehension(condition: bool, values: list[bool]):
    while condition:  # no diagnostic
        [(condition := value) for value in values]

def handler(condition: bool):
    while condition:  # error: [possibly-unresolved-reference]
        try:
            print("waiting")
        except Exception as condition:  # error: [invalid-assignment]
            pass
```

Assignments outside the loop do not prevent a diagnostic.

```py
def sequential(condition: bool):
    condition = bool(input())
    while condition:  # error: [invariant-while-condition]
        print("waiting")
    condition = False

def enclosing(values: list[bool]):
    for condition in values:
        while condition:  # error: [invariant-while-condition]
            print("waiting")
```

## Other scopes

Global variables and captured variables can be reassigned by other code.

```py
condition: bool = bool(input())

while condition:  # no diagnostic
    print("waiting")

def global_condition():
    while condition:  # no diagnostic
        print("waiting")

def outer(condition: bool):
    def inner():
        while condition:  # no diagnostic
            print("waiting")
```

A nested function can update a local variable through `nonlocal`, including when it is defined
inside the loop.

```py
def outer(condition: bool):
    def update():
        nonlocal condition
        condition = bool(input())

    while condition:  # no diagnostic
        update()

def outer(condition: bool):
    while condition:  # no diagnostic
        def update():
            nonlocal condition
            condition = bool(input())
        update()
```

Writes to an unrelated global variable do not change a local variable with the same name.

```py
def outer(condition: bool):
    def update():
        global condition
        condition = bool(input())

    while condition:  # error: [invariant-while-condition]
        update()
```

## Mutable values and repeated evaluation

An unchanged local binding can refer to an object whose truthiness changes.

```py
from collections.abc import Callable

class Mutable:
    def __bool__(self) -> bool:
        return bool(input())

class MutableInt(int):
    def __bool__(self) -> bool:
        return bool(input())

def objects(value: Mutable):
    while value:  # no diagnostic
        print("waiting")

def subclass(value: int):
    while value:  # no diagnostic
        print("waiting")

def collection(values: list[int]):
    while values:  # no diagnostic
        values.pop()

def call(condition: Callable[[], bool]):
    while condition():  # no diagnostic
        print("waiting")

class State:
    condition: bool

def attribute(state: State):
    while state.condition:  # no diagnostic
        print("waiting")

def subscript(state: list[bool]):
    while state[0]:  # no diagnostic
        print("waiting")

def walrus():
    while condition := bool(input()):  # no diagnostic
        print(condition)
```

## Loop exits

A loop with an unchanged condition can still terminate through an explicit exit in its body. We
report the invariant condition regardless of whether the loop can terminate.

```py
from typing_extensions import Never

def stop() -> Never:
    raise RuntimeError

def with_break(condition: bool):
    while condition:  # error: [invariant-while-condition]
        if input():
            break

def with_return(condition: bool):
    while condition:  # error: [invariant-while-condition]
        if input():
            return

def with_raise(condition: bool):
    while condition:  # error: [invariant-while-condition]
        raise RuntimeError

def with_assert(condition: bool):
    while condition:  # error: [invariant-while-condition]
        assert input()

def never_call(condition: bool):
    while condition:  # error: [invariant-while-condition]
        stop()

async def async_stop() -> Never:
    raise RuntimeError

async def awaited_never(condition: bool):
    while condition:  # error: [invariant-while-condition]
        await async_stop()

def comprehension_exit(condition: bool):
    while condition:  # error: [invariant-while-condition]
        [stop() for _ in range(3)]

def default_exit(condition: bool):
    while condition:  # error: [invariant-while-condition]
        callback = lambda value=stop(): value

def generator_iterable_exit(condition: bool):
    while condition:  # error: [invariant-while-condition]
        values = (value for value in stop())
```

A `continue` does not exit the loop. A `break` in a nested loop exits only that nested loop, whereas
a `break` in its `else` suite exits the outer loop.

```py
def with_continue(condition: bool):
    while condition:  # error: [invariant-while-condition]
        continue

def nested_break(condition: bool):
    while condition:  # error: [invariant-while-condition]
        for item in range(3):
            break

def else_break(condition: bool):
    while condition:  # error: [invariant-while-condition]
        for item in range(3):
            print(item)
        else:
            break
```

Returning from a nested function does not exit the loop that defines it.

```py
def nested_return(condition: bool):
    while condition:  # error: [invariant-while-condition]
        def get_value():
            return 42
        print(get_value())
```

## Fixed truthiness and literals

Literal conditions are intentional. Other conditions with statically known truthiness are left to
the redundant-condition rules.

```toml
[rules]
redundant-condition-strict = "warn"
```

```py
def literals():
    while True:  # no diagnostic
        pass
    while False:  # no diagnostic
        pass
    while 1:  # no diagnostic
        pass
    while 0:  # no diagnostic
        pass

def fixed_true():
    condition = True
    while condition:  # error: [redundant-condition-strict]
        print("waiting")

def fixed_false():
    condition = False
    while condition:  # error: [redundant-condition-strict]
        print("waiting")
```

## Disabled rule

```toml
[rules]
invariant-while-condition = "ignore"
```

```py
def wait(condition: bool):
    while condition:  # no diagnostic
        print("waiting")
```
