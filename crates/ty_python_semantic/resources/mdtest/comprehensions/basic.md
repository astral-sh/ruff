# Comprehensions

## Basic comprehensions

```py
# revealed: int
[reveal_type(x) for x in range(3)]

class Row:
    def __next__(self) -> range:
        return range(3)

class Table:
    def __iter__(self) -> Row:
        return Row()

# revealed: tuple[int, range]
[reveal_type((cell, row)) for row in Table() for cell in row]

# revealed: int
{reveal_type(x): 0 for x in range(3)}

# revealed: int
{0: reveal_type(x) for x in range(3)}
```

## Nested comprehension

```py
# revealed: tuple[int, int]
[[reveal_type((x, y)) for x in range(3)] for y in range(3)]
```

## Assignment expressions in comprehensions

[PEP 572] specifies that an assignment expression in a comprehension binds its target in the scope
containing the outermost comprehension.

ty currently assumes that a comprehension runs at least once. It also analyzes a generator
expression as though it is consumed immediately. The tests below follow those existing assumptions.

### Basic forms

Assignment expressions can appear in the element of a list comprehension and in the key or value of
a dictionary comprehension:

```py
[(list_value := item) for item in [1]]
{(dict_key := item): (dict_value := item) for item in [1]}

reveal_type(list_value)  # revealed: int
reveal_type(dict_key)  # revealed: int
reveal_type(dict_value)  # revealed: int
```

### Generator expressions

The target also binds in the containing scope when the assignment is in a generator expression. PEP
572 uses this `any` pattern as a motivating example:

```py
def find_comment(lines: list[str]):
    if any((comment := line).startswith("#") for line in lines):
        reveal_type(comment)  # revealed: str
```

### Assignment order

If an iteration assigns the same target more than once, the last assignment determines its value
after the comprehension:

```py
[(ordered := item, ordered := "") for item in [1]]
reveal_type(ordered)  # revealed: str
```

### Branches that do not assign

A target in a branch known not to run remains unbound. A target in the branch that does run is
available after the comprehension:

```py
[(dead := 1) if False else (live := 2) for _ in [0]]

dead  # error: [unresolved-reference]
reveal_type(live)  # revealed: int
```

### Assignments on only some paths

When the assignment only runs on one possible path, an earlier value remains possible:

```py
def conditional_with_previous_value(flag: bool):
    value = "old"
    [(value := 1) if flag else 0 for _ in [0]]
    reveal_type(value)  # revealed: Literal["old"] | int
```

Without an earlier value, the target may be unbound:

```py
def conditional_without_previous_value(flag: bool):
    [(value := 1) if flag else 0 for _ in [0]]
    # error: [possibly-unresolved-reference]
    reveal_type(value)  # revealed: int
```

### Comprehension filters

A false filter skips the element, but an assignment made while evaluating that filter still takes
effect:

```py
[value for value in [True, False] if (last_value := value)]
reveal_type(last_value)  # revealed: bool
```

If short-circuit evaluation skips the assignment, the target may be unbound:

```py
def conditional_filter(flag: bool):
    [0 for _ in [0] if flag and (value := 1)]
    # error: [possibly-unresolved-reference]
    reveal_type(value)  # revealed: int
```

An assignment in the element only runs when every preceding filter succeeds:

```py
def assignment_after_filter(flag: bool):
    [(value := 1) for _ in [0] if flag]
    # error: [possibly-unresolved-reference]
    reveal_type(value)  # revealed: int
```

### Assignments that depend on earlier iterations

An assignment can read the value left by an earlier iteration. In this example, the final value is
`3`, so retaining only the first iteration's literal values would be incorrect:

```py
def partial_sum():
    total = 0
    [total := total + value for value in [1, 2]]
    reveal_type(total)  # revealed: int
```

The same applies when two targets depend on values from earlier iterations:

```py
def two_dependent_targets():
    x = 0
    y = 0
    [(y := x, x := y + 1) for _ in [1, 2]]
    reveal_type(x)  # revealed: int
    reveal_type(y)  # revealed: int
```

### Function-local targets

Even if the assignment is in a branch known not to run, its target belongs to the containing
function. A read in that function must not fall back to a global variable with the same name:

```py
local_target = "global"

def read_local_target():
    [(local_target := 1) if False else 0 for _ in [0]]
    local_target  # error: [unresolved-reference]
```

### Nested comprehensions

An assignment in an inner comprehension still binds outside the outermost comprehension. The order
of assignments in the outer comprehension is preserved:

```py
[([nested_order := 1 for _ in [0]], (nested_order := 2)) for _ in [0]]
reveal_type(nested_order)  # revealed: int
```

An inner comprehension that is never evaluated must not replace an earlier value:

```py
def unreachable_nested_assignment_with_previous_value():
    value = "old"
    [[value := 1 for _ in [0]] if False else [] for _ in [0]]
    reveal_type(value)  # revealed: Literal["old"]
```

Nor should it create a new value:

```py
def unreachable_nested_assignment_without_previous_value():
    [[value := 1 for _ in [0]] if False else [] for _ in [0]]
    value  # error: [unresolved-reference]
```

## Comprehension referencing outer comprehension

```py
class Row:
    def __next__(self) -> range:
        return range(3)

class Table:
    def __iter__(self) -> Row:
        return Row()

# revealed: tuple[int, range]
[[reveal_type((cell, row)) for cell in row] for row in Table()]
```

## Comprehension with unbound iterable

Iterating over an unbound iterable yields `Unknown`:

```py
# error: [unresolved-reference] "Name `x` used when not defined"
# revealed: Unknown
[reveal_type(z) for z in x]

# error: [not-iterable] "Object of type `int` is not iterable"
# revealed: tuple[int, Unknown]
[reveal_type((x, z)) for x in range(3) for z in x]

# error: [unresolved-reference] "Name `foo` used when not defined"
foo
foo = [
    # revealed: tuple[int, Unknown]
    reveal_type((x, z))
    for x in range(3)
    # error: [unresolved-reference] "Name `foo` used when not defined"
    for z in [foo]
]

baz = [
    # revealed: tuple[int, Unknown]
    reveal_type((x, z))
    for x in range(3)
    # error: [unresolved-reference] "Name `baz` used when not defined"
    for z in [baz]
]
```

## Starred expressions

Starred expressions must be iterable

```py
class NotIterable: ...

# This is fine:
x = [*range(3)]

# error: [not-iterable] "Object of type `NotIterable` is not iterable"
y = [*NotIterable()]
```

## Async comprehensions

### Basic

```py
class AsyncIterator:
    async def __anext__(self) -> int:
        return 42

class AsyncIterable:
    def __aiter__(self) -> AsyncIterator:
        return AsyncIterator()

async def _():
    # revealed: int
    [reveal_type(x) async for x in AsyncIterable()]
```

### Invalid async comprehension

This tests that we understand that `async` comprehensions do *not* work according to the synchronous
iteration protocol

```py
async def _():
    # error: [not-iterable] "Object of type `range` is not async-iterable"
    # revealed: Unknown
    [reveal_type(x) async for x in range(3)]
```

## Comprehension value type

The type of the expression being iterated over is immutable, and so should not be widened with
`Unknown` or through literal promotion:

```py
x = [
    reveal_type(string)  # revealed: Literal["a", "b"]
    for string in ["a", "b"]
]
```

## Comprehension expression types

The type of the comprehension expression itself should reflect the inferred element type:

```py
from typing import TypedDict, Literal

# revealed: list[int]
reveal_type([x for x in range(10)])

# revealed: set[int]
reveal_type({x for x in range(10)})

# revealed: dict[int, str]
reveal_type({x: str(x) for x in range(10)})

# revealed: list[tuple[int, str]]
reveal_type([(x, y) for x in range(5) for y in ["a", "b", "c"]])

squares: list[int | None] = [x**2 for x in range(10)]
reveal_type(squares)  # revealed: list[int | None]
```

## PEP 798 unpacking comprehensions

```toml
[environment]
python-version = "3.15"
```

Unpacking comprehensions flatten the unpacked element type:

```py
list_of_lists: list[list[int]] = [[1], [2, 3]]
sets: list[set[str]] = [{"a"}, {"b", "c"}]
dicts: list[dict[str, int]] = [{"a": 1}, {"b": 2}]
not_iterables: list[int] = [1, 2]

reveal_type([*xs for xs in list_of_lists])  # revealed: list[int]
reveal_type({*xs for xs in sets})  # revealed: set[str]
reveal_type({**d for d in dicts})  # revealed: dict[str, int]

[*value for value in not_iterables]  # error: [not-iterable] "Object of type `int` is not iterable"
{*value for value in not_iterables}  # error: [not-iterable] "Object of type `int` is not iterable"
{**value for value in not_iterables}  # error: [invalid-argument-type]
```

## Inference for comprehensions takes context

Inference for comprehensions takes the type context into account:

```py
from typing import Literal, Sequence, TypedDict

# Without type context:
reveal_type([x for x in [1, 2, 3]])  # revealed: list[int]
reveal_type({x: "a" for x in [1, 2, 3]})  # revealed: dict[int, str]
reveal_type({str(x): x for x in [1, 2, 3]})  # revealed: dict[str, int]
reveal_type({x for x in [1, 2, 3]})  # revealed: set[int]

# With type context:
x1: list[int] = [x for x in [1, 2, 3]]
reveal_type(x1)  # revealed: list[int]

x2: Sequence[int] = [x for x in [1, 2, 3]]
reveal_type(x2)  # revealed: list[int]

x3: dict[int, str] = {x: str(x) for x in [1, 2, 3]}
reveal_type(x3)  # revealed: dict[int, str]

x4: set[int] = {x for x in [1, 2, 3]}
reveal_type(x4)  # revealed: set[int]
```

This also works for nested comprehensions:

```py
table = [[(x, y) for x in range(3)] for y in range(3)]
reveal_type(table)  # revealed: list[list[tuple[int, int]]]

table_with_content: list[list[tuple[int, int, str | None]]] = [[(x, y, None) for x in range(3)] for y in range(3)]
reveal_type(table_with_content)  # revealed: list[list[tuple[int, int, str | None]]]
```

The type context is propagated down into the comprehension:

```py
y1: list[list[int]] = [[n] for n in [1, 2, 3]]
reveal_type(y1)  # revealed: list[list[int]]

y2: list[Sequence[int]] = [[i] for i in [1, 2, 3]]
reveal_type(y2)  # revealed: list[Sequence[int]]

class Person(TypedDict):
    name: str

y3: list[Person] = [{"name": n} for n in ["Alice", "Bob"]]
reveal_type(y3)  # revealed: list[Person]

# error: [invalid-assignment]
# error: [invalid-key] "Unknown key "misspelled" for TypedDict `Person`"
# error: [missing-typed-dict-key] "Missing required key 'name' in TypedDict `Person` constructor"
y4: list[Person] = [{"misspelled": n} for n in ["Alice", "Bob"]]
```

We promote literals to avoid overly-precise types in invariant positions:

```py
reveal_type([x for x in ("a", "b", "c")])  # revealed: list[str]
reveal_type({x for x in (1, 2, 3)})  # revealed: set[int]
reveal_type({k: 0 for k in ("a", "b", "c")})  # revealed: dict[str, int]
```

Type context can prevent this promotion from happening:

```py
list_of_literals: list[Literal["a", "b", "c"]] = [x for x in ("a", "b", "c")]
reveal_type(list_of_literals)  # revealed: list[Literal["a", "b", "c"]]

dict_with_literal_keys: dict[Literal["a", "b", "c"], int] = {k: 0 for k in ("a", "b", "c")}
reveal_type(dict_with_literal_keys)  # revealed: dict[Literal["a", "b", "c"], int]

dict_with_literal_values: dict[str, Literal[1, 2, 3]] = {str(k): k for k in (1, 2, 3)}
reveal_type(dict_with_literal_values)  # revealed: dict[str, Literal[1, 2, 3]]

set_with_literals: set[Literal[1, 2, 3]] = {k for k in (1, 2, 3)}
reveal_type(set_with_literals)  # revealed: set[Literal[1, 2, 3]]
```

[pep 572]: https://peps.python.org/pep-0572/#scope-of-the-target
