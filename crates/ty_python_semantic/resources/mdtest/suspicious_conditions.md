# Suspicious boolean conditions

This document tests the `truthiness-test-of-callable` and `truthiness-test-of-iterable` rules. The
first warns when a value whose type is `Callable` or a union of callables is tested in a boolean
condition without being called. The second warns when a value typed as `Iterable`, `Iterator`, or
`Generator` is tested for truthiness: a value with one of these types can often be a generator, and
a generator is always truthy even if it is empty.

Whereas `redundant-condition` and `redundant-condition-strict` rules report conditions that ty
infers as always truthy or always falsy, the rules tested here flag conditions that have ambiguous
truthiness but are nonetheless highly suspicious, and probably indicative of buggy code.

## Callable values

Testing a `Callable` does not invoke it. We suggest a call when the value takes no arguments:

```py
from typing import Callable

def check(predicate: Callable[[], bool]):
    if predicate:  # snapshot: truthiness-test-of-callable
        pass
    if predicate():  # no diagnostic
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
 --> src/mdtest_snippet.py:4:8
  |
4 |     if predicate:  # snapshot: truthiness-test-of-callable
  |        ^^^^^^^^^ Has type `() -> bool`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `predicate()`
  |
3 | def check(predicate: Callable[[], bool]):
  -     if predicate:  # snapshot: truthiness-test-of-callable
4 +     if predicate():  # snapshot: truthiness-test-of-callable
5 |         pass
  |
note: This is an unsafe fix and may change runtime behavior
```

When arguments are required, the suggested call leaves them for the user to supply:

```py
def check_with_argument(predicate: Callable[[int], bool]):
    if predicate:  # snapshot: truthiness-test-of-callable
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
 --> src/mdtest_snippet.py:9:8
  |
9 |     if predicate:  # snapshot: truthiness-test-of-callable
  |        ^^^^^^^^^ Has type `(int, /) -> bool`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `predicate(...)`
   |
8  | def check_with_argument(predicate: Callable[[int], bool]):
   -     if predicate:  # snapshot: truthiness-test-of-callable
9  +     if predicate(...):  # snapshot: truthiness-test-of-callable
10 |         pass
   |
note: This is a display-only fix and is likely to be incorrect
```

## Unions of callable values

A union of callables still represents callable objects. A union containing `None` may instead be
testing whether the value is present:

```py
from typing import Callable

def check_union(
    predicate: Callable[[int], bool] | Callable[[str], bool],
    optional: Callable[[], bool] | None,
):
    if predicate:  # error: [truthiness-test-of-callable]
        pass
    if optional:  # no diagnostic
        optional()
```

If >=1 `Callable` type in a union has parameters, the autofix adding a call is marked as
display-only rather than unsafe:

```py
def check_union2(predicate: Callable[[], bool] | Callable[[str], bool]):
    if predicate:  # snapshot: truthiness-test-of-callable
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
  --> src/mdtest_snippet.py:12:8
   |
12 |     if predicate:  # snapshot: truthiness-test-of-callable
   |        ^^^^^^^^^ Has type `(() -> bool) | ((str, /) -> bool)`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `predicate(...)`
   |
11 | def check_union2(predicate: Callable[[], bool] | Callable[[str], bool]):
   -     if predicate:  # snapshot: truthiness-test-of-callable
12 +     if predicate(...):  # snapshot: truthiness-test-of-callable
13 |         pass
   |
note: This is a display-only fix and is likely to be incorrect
```

and the same is currently true for callables with the gradual parameter list (`...`), though
arguably an unsafe autofix would be equally fine here:

```py
# error: [missing-type-argument] "for generic type `Callable`"
def check_gradual(predicate1: Callable[..., bool], predicate2: Callable):
    if predicate1:  # snapshot: truthiness-test-of-callable
        pass
    
    if predicate2:  # snapshot: truthiness-test-of-callable
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
  --> src/mdtest_snippet.py:16:8
   |
16 |     if predicate1:  # snapshot: truthiness-test-of-callable
   |        ^^^^^^^^^^ Has type `(...) -> bool`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `predicate1(...)`
   |
15 | def check_gradual(predicate1: Callable[..., bool], predicate2: Callable):
   -     if predicate1:  # snapshot: truthiness-test-of-callable
16 +     if predicate1(...):  # snapshot: truthiness-test-of-callable
17 |         pass
   |
note: This is a display-only fix and is likely to be incorrect


warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
  --> src/mdtest_snippet.py:19:8
   |
19 |     if predicate2:  # snapshot: truthiness-test-of-callable
   |        ^^^^^^^^^^ Has type `(...) -> Unknown`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `predicate2(...)`
   |
18 |     
   -     if predicate2:  # snapshot: truthiness-test-of-callable
19 +     if predicate2(...):  # snapshot: truthiness-test-of-callable
20 |         pass
   |
note: This is a display-only fix and is likely to be incorrect
```

## Awaitable return values

Calling a callable that returns a coroutine turns the boolean test from a suspicious one into one
that tests an always-truthy value. That's also probably not what the user intended, so, in an
asynchronous function, we suggest additionally awaiting the result of the call where it's
appropriate:

```py
from typing import Any, Callable, Coroutine

async def check(predicate: Callable[[], Coroutine[Any, Any, bool]]):
    if predicate:  # snapshot: truthiness-test-of-callable
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
 --> src/mdtest_snippet.py:4:8
  |
4 |     if predicate:  # snapshot: truthiness-test-of-callable
  |        ^^^^^^^^^ Has type `() -> Coroutine[Any, Any, bool]`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call and await this callable?
help: Replace with `await predicate()`
  |
3 | async def check(predicate: Callable[[], Coroutine[Any, Any, bool]]):
  -     if predicate:  # snapshot: truthiness-test-of-callable
4 +     if await predicate():  # snapshot: truthiness-test-of-callable
5 |         pass
  |
note: This is an unsafe fix and may change runtime behavior
```

## Iterable values

Testing an `Iterable` for truthiness is a common footgun, since a variable typed as an `Iterable`
could be a generator, in which case it would be truthy even if it were empty:

```py
from typing import Iterable

# error: [missing-type-argument] "for generic class `Iterable`"
def check_items(items: Iterable[int], items2: Iterable):
    if items:  # snapshot: truthiness-test-of-iterable
        pass
    
    if items2:  # error: [truthiness-test-of-iterable]
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:5:8
  |
5 |     if items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^ Has type `Iterable[int]`
info: Iterable objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
  |
4 | def check_items(items: Iterable[int], items2: Iterable):
  -     if items:  # snapshot: truthiness-test-of-iterable
5 +     if len(tuple(items)):  # snapshot: truthiness-test-of-iterable
6 |         pass
  |
note: This is a display-only fix and is likely to be incorrect
```

A `Collection`-typed variable, on the other hand, cannot be a generator, and the `Collection`
protocol can only be inhabited by objects that define `__len__`, so we emit no diagnostic here:

```py
from typing import Collection

def check2(items: Collection[int]):
    if items:  # no diagnostic
        pass
```

`Iterator` and `Generator` annotations can also describe empty generators:

```py
from typing import Generator, Iterator

# error: [missing-type-argument] "for generic class `Iterator`"
# error: [missing-type-argument] "for generic class `Generator`"
def check_iterators(iterator: Iterator[int], iterator2: Iterator, generator: Generator[int, None, None], generator2: Generator):
    if iterator:  # error: [truthiness-test-of-iterable]
        pass
    if generator:  # error: [truthiness-test-of-iterable]
        pass
    if iterator2:  # error: [truthiness-test-of-iterable]
        pass
    if generator2:  # error: [truthiness-test-of-iterable]
        pass
```

So can custom protocols that are subtypes of `Iterable[object]` but supertypes of all
`GeneratorType` specializations:

```py
from typing import Protocol, Iterator

class GeneratorLike(Protocol):
    def __iter__(self) -> Iterator[int]: ...
    @property
    def gi_running(self) -> bool: ...

def check_custom(x: GeneratorLike):
    if x:  # error: [truthiness-test-of-iterable]
        pass
```

and some intersection types:

```py
from ty_extensions import Intersection, Not

class Unrelated: ...

def check_intersection(x: Intersection[GeneratorLike, Not[Unrelated]]):
    if x:  # error: [truthiness-test-of-iterable]
        pass
```

and even certain unions:

```py
from typing import Mapping

def check_unions(x: Iterable[int] | Mapping[str, str], y: Intersection[GeneratorLike, Not[Unrelated]] | Mapping[str, str]):
    # revealed: Iterable[int] | Mapping[str, str]
    if reveal_type(x):  # error: [truthiness-test-of-iterable]
        pass

    # revealed: (GeneratorLike & ~Unrelated) | Mapping[str, str]
    if reveal_type(y):  # error: [truthiness-test-of-iterable]
        pass
```

## Nested boolean tests

Each tested operand receives the appropriate diagnostic, even when the whole condition remains
ambiguous:

```py
from collections.abc import Callable, Iterable

def check_nested(flag: bool, predicate: Callable[[], bool], items: Iterable[int]):
    if flag and predicate:  # snapshot: truthiness-test-of-callable
        pass
    if flag and items:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
 --> src/mdtest_snippet.py:4:17
  |
4 |     if flag and predicate:  # snapshot: truthiness-test-of-callable
  |                 ^^^^^^^^^ Has type `() -> bool`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `predicate()`
  |
3 | def check_nested(flag: bool, predicate: Callable[[], bool], items: Iterable[int]):
  -     if flag and predicate:  # snapshot: truthiness-test-of-callable
4 +     if flag and predicate():  # snapshot: truthiness-test-of-callable
5 |         pass
  |
note: This is an unsafe fix and may change runtime behavior


warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:6:17
  |
6 |     if flag and items:  # snapshot: truthiness-test-of-iterable
  |                 ^^^^^ Has type `Iterable[int]`
info: Iterable objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
  |
5 |         pass
  -     if flag and items:  # snapshot: truthiness-test-of-iterable
6 +     if flag and len(tuple(items)):  # snapshot: truthiness-test-of-iterable
7 |         pass
  |
note: This is a display-only fix and is likely to be incorrect
```

When both operands are suspicious, we report each tested operand without also warning on the
complete `or` expression:

```py
def check_two_predicates(first: Callable[[], bool], second: Callable[[], bool]):
    # snapshot: truthiness-test-of-callable
    # snapshot: truthiness-test-of-callable
    if first or second:
        pass

def check_two_iterables(left: Iterable[int], right: Iterable[int]):
    # error: [truthiness-test-of-iterable]
    # error: [truthiness-test-of-iterable]
    if left or right:
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
  --> src/mdtest_snippet.py:11:8
   |
11 |     if first or second:
   |        ^^^^^ Has type `() -> bool`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `first()`
   |
10 |     # snapshot: truthiness-test-of-callable
   -     if first or second:
11 +     if first() or second:
12 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
  --> src/mdtest_snippet.py:11:17
   |
11 |     if first or second:
   |                 ^^^^^^ Has type `() -> bool`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `second()`
   |
10 |     # snapshot: truthiness-test-of-callable
   -     if first or second:
11 +     if first or second():
12 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

When a condition tests a callable and an iterable, both rules report their respective operands:

```py
def check_mixed(predicate: Callable[[], bool], items: Iterable[int]):
    # snapshot: truthiness-test-of-callable
    # snapshot: truthiness-test-of-iterable
    if predicate and items:
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
  --> src/mdtest_snippet.py:22:8
   |
22 |     if predicate and items:
   |        ^^^^^^^^^ Has type `() -> bool`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `predicate()`
   |
21 |     # snapshot: truthiness-test-of-iterable
   -     if predicate and items:
22 +     if predicate() and items:
23 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:22:22
   |
22 |     if predicate and items:
   |                      ^^^^^ Has type `Iterable[int]`
info: Iterable objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
   |
21 |     # snapshot: truthiness-test-of-iterable
   -     if predicate and items:
22 +     if predicate and len(tuple(items)):
23 |         pass
   |
note: This is a display-only fix and is likely to be incorrect
```

## Suspicious operands in fixed conditions

A `Callable` or `Iterable` expression has ambiguous truthiness, but nesting such an expression
inside `and False` means the outer expression will always be falsy. The operand's suspicious type
and the fixed truthiness of the complete condition are reported as separate findings. The diagnostic
on the complete condition also explains why the body is unreachable:

```toml
[rules]
redundant-condition-strict = "error"
```

```py
from collections.abc import Callable, Iterable

def check(predicate: Callable[[], bool], items: Iterable[int]):
    # snapshot: truthiness-test-of-callable
    # snapshot: redundant-condition-strict
    if predicate and False:
        print("unreachable")

    # error: [truthiness-test-of-iterable]
    # error: [redundant-condition-strict] "Condition `items and False` is always false"
    if items and False:
        pass
```

```snapshot
error[redundant-condition-strict]: Condition is always false
 --> src/mdtest_snippet.py:6:8
  |
6 |     if predicate and False:
  |        ^^^^^^^^^^^^^^^^^^^ Inferred type is `((() -> bool) & ~AlwaysTruthy) | Literal[False]`
7 |         print("unreachable")
  |         -------------------- This statement is unreachable


warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
 --> src/mdtest_snippet.py:6:8
  |
6 |     if predicate and False:
  |        ^^^^^^^^^ Has type `() -> bool`
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `predicate()`
  |
5 |     # snapshot: redundant-condition-strict
  -     if predicate and False:
6 +     if predicate() and False:
7 |         print("unreachable")
  |
note: This is an unsafe fix and may change runtime behavior
```

The suspicious operand can also be nested inside an expression whose own truthiness is ambiguous.
The outer `and False` still has a fixed outcome:

```py
def check_nested_fixed(predicate: Callable[[], bool]):
    # error: [truthiness-test-of-callable]
    # error: [redundant-condition-strict]
    if (not predicate) and False:
        pass
```

A truthiness test inside a tuple will be independent of the test of the tuple itself, so both the
suspicious operand and the always-truthy tuple can be reported:

```py
def check_tuple(predicate: Callable[[], bool]):
    # error: [truthiness-test-of-callable]
    # error: [redundant-condition]
    if (not predicate,):
        pass
```
