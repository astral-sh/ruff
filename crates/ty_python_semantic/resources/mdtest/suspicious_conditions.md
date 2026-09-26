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
  |        ^^^^^^^^^ `() -> bool` object tested for truthiness
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
  |        ^^^^^^^^^ `(int, /) -> bool` object tested for truthiness
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
   |        ^^^^^^^^^ `(() -> bool) | ((str, /) -> bool)` object tested for truthiness
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
   |        ^^^^^^^^^^ `(...) -> bool` object tested for truthiness
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
   |        ^^^^^^^^^^ `(...) -> Unknown` object tested for truthiness
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

## Unions with functions and bound methods

A union of a `Callable` type and a function-literal function is still suspicious in a boolean test.
The function is always truthy, and testing the selected value does not call it:

```py
from typing import Callable

def default() -> str:
    return "ready"

def check(callback: Callable[[], bool], flag: bool):
    selected = callback if flag else default
    reveal_type(selected)  # revealed: (() -> bool) | (def default() -> str)
    if selected:  # error: [truthiness-test-of-callable]
        pass
```

Bound methods are also always truthy. The suggested call does not need an argument for `self`, since
the method is already bound to its instance:

```py
class Handler:
    def ready(self) -> str:
        return "ready"

def check_method(callback: Callable[[], bool], handler: Handler, flag: bool):
    selected = callback if flag else handler.ready
    reveal_type(selected)  # revealed: (() -> bool) | (bound method Handler.ready() -> str)
    if selected:  # snapshot: truthiness-test-of-callable
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
  --> src/mdtest_snippet.py:18:8
   |
18 |     if selected:  # snapshot: truthiness-test-of-callable
   |        ^^^^^^^^ `(() -> bool) | (bound method Handler.ready() -> str)` object tested for truthiness
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `selected()`
   |
17 |     reveal_type(selected)  # revealed: (() -> bool) | (bound method Handler.ready() -> str)
   -     if selected:  # snapshot: truthiness-test-of-callable
18 +     if selected():  # snapshot: truthiness-test-of-callable
19 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

## Unions with callable instances

An always-truthy instance remains suspicious when its `__call__` attribute has several possible
signatures. If any signature requires an argument, the suggested call leaves it for the user to
supply:

```py
from typing import final, Callable

@final
class CallbackAlternatives:
    __call__: Callable[["CallbackAlternatives"], str] | Callable[["CallbackAlternatives", int], bytes]

def check_alternatives(callback: Callable[[], bool] | CallbackAlternatives):
    if callback:  # snapshot: truthiness-test-of-callable
        pass
```

```snapshot
warning[truthiness-test-of-callable]: Suspicious boolean test of a `Callable`
 --> src/mdtest_snippet.py:8:8
  |
8 |     if callback:  # snapshot: truthiness-test-of-callable
  |        ^^^^^^^^ `(() -> bool) | CallbackAlternatives` object tested for truthiness
info: Callable objects are usually functions, and functions are always truthy
help: Did you mean to call this callable?
help: Replace with `callback(...)`
  |
7 | def check_alternatives(callback: Callable[[], bool] | CallbackAlternatives):
  -     if callback:  # snapshot: truthiness-test-of-callable
8 +     if callback(...):  # snapshot: truthiness-test-of-callable
9 |         pass
  |
note: This is a display-only fix and is likely to be incorrect
```

## Callable type aliases

Type aliases do not change whether a callable's truthiness test is suspicious. Both legacy aliases
and PEP 695 aliases receive the same warning as their underlying callable types, including aliases
of unions and aliases used within unions:

```toml
[environment]
python-version = "3.12"
```

```py
from collections.abc import Callable
from typing import TypeAlias

LegacyCallback: TypeAlias = Callable[[], bool]
type Callback = Callable[[], bool]
type Callbacks = Callback | Callable[[int], bool]

def check(
    legacy: LegacyCallback,
    callback: Callback,
    callbacks: Callbacks,
    mixed: Callbacks | Callable[[str], bool],
):
    if legacy:  # error: [truthiness-test-of-callable]
        pass
    if callback:  # error: [truthiness-test-of-callable]
        pass
    if callbacks:  # error: [truthiness-test-of-callable]
        pass
    if mixed:  # error: [truthiness-test-of-callable]
        pass
```

An alias that includes `None` can still be tested to check whether a callback is present:

```py
type OptionalCallback = Callback | None

def check_optional(callback: OptionalCallback):
    if callback:
        callback()
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
  |        ^^^^^^^^^ `() -> Coroutine[Any, Any, bool]` object tested for truthiness
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
4 | def check_items(items: Iterable[int], items2: Iterable):
  |                        ------------- Inferred as `Iterable[int]` due to this annotation
5 |     if items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[int]` as the annotation on line 4
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
1 + import collections.abc
2 | from typing import Iterable
3 |
4 | # error: [missing-type-argument] "for generic class `Iterable`"
  - def check_items(items: Iterable[int], items2: Iterable):
5 + def check_items(items: collections.abc.Collection[int], items2: Iterable):
6 |     if items:  # snapshot: truthiness-test-of-iterable
  |
note: This is an unsafe fix and may change runtime behavior
```

A `Collection`-typed variable, on the other hand, cannot be a generator, and the `Collection`
protocol can only be inhabited by objects that define `__len__`, so we emit no diagnostic here:

```py
from typing import Collection

def check2(items: Collection[int]):
    if items:  # no diagnostic
        pass
```

`Iterator` can also describe empty generators. We only suggest replacing an annotation with
`Collection` when it describes an `Iterable`: `Collection` preserves all operations that `Iterable`
provides, but it does not provide the `__next__` method required by `Iterator`. Replacing `Iterator`
with `Collection` would therefore make the call to `next(iterator)` below invalid:

```py
from typing import Iterator

# error: [missing-type-argument] "for generic class `Iterator`"
def check_iterators(iterator: Iterator[int], iterator2: Iterator):
    if iterator:  # snapshot: truthiness-test-of-iterable
        next(iterator)
    if iterator2:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:19:8
   |
18 | def check_iterators(iterator: Iterator[int], iterator2: Iterator):
   |                               ------------- Inferred as `Iterator[int]` due to this annotation
19 |     if iterator:  # snapshot: truthiness-test-of-iterable
   |        ^^^^^^^^ `Iterator[int]` object tested for truthiness
info: `Iterator[int]` objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
   |
18 | def check_iterators(iterator: Iterator[int], iterator2: Iterator):
   -     if iterator:  # snapshot: truthiness-test-of-iterable
19 +     if len(tuple(iterator)):  # snapshot: truthiness-test-of-iterable
20 |         next(iterator)
   |
note: This is a display-only fix and is likely to be incorrect


warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:21:8
   |
18 | def check_iterators(iterator: Iterator[int], iterator2: Iterator):
   |                                                         -------- Inferred as `Iterator[Unknown]` due to this annotation
19 |     if iterator:  # snapshot: truthiness-test-of-iterable
20 |         next(iterator)
21 |     if iterator2:  # snapshot: truthiness-test-of-iterable
   |        ^^^^^^^^^ `Iterator[Unknown]` object tested for truthiness
info: `Iterator[Unknown]` objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
   |
20 |         next(iterator)
   -     if iterator2:  # snapshot: truthiness-test-of-iterable
21 +     if len(tuple(iterator2)):  # snapshot: truthiness-test-of-iterable
22 |         pass
   |
note: This is a display-only fix and is likely to be incorrect
```

as can `Generator`:

```py
from typing import Generator

# error: [missing-type-argument] "for generic class `Generator`"
def check_generators(generator: Generator[int, None, None], generator2: Generator):
    if generator:  # snapshot: truthiness-test-of-iterable
        pass
    if generator2:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:27:8
   |
26 | def check_generators(generator: Generator[int, None, None], generator2: Generator):
   |                                 -------------------------- Inferred as `Generator[int, None, None]` due to this annotation
27 |     if generator:  # snapshot: truthiness-test-of-iterable
   |        ^^^^^^^^^ `Generator[int, None, None]` object tested for truthiness
info: `Generator[int, None, None]` objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
   |
26 | def check_generators(generator: Generator[int, None, None], generator2: Generator):
   -     if generator:  # snapshot: truthiness-test-of-iterable
27 +     if len(tuple(generator)):  # snapshot: truthiness-test-of-iterable
28 |         pass
   |
note: This is a display-only fix and is likely to be incorrect


warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:29:8
   |
26 | def check_generators(generator: Generator[int, None, None], generator2: Generator):
   |                                                                         --------- Inferred as `Generator[Unknown, None, None]` due to this annotation
27 |     if generator:  # snapshot: truthiness-test-of-iterable
28 |         pass
29 |     if generator2:  # snapshot: truthiness-test-of-iterable
   |        ^^^^^^^^^^ `Generator[Unknown, None, None]` object tested for truthiness
info: `Generator[Unknown, None, None]` objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
   |
28 |         pass
   -     if generator2:  # snapshot: truthiness-test-of-iterable
29 +     if len(tuple(generator2)):  # snapshot: truthiness-test-of-iterable
30 |         pass
   |
note: This is a display-only fix and is likely to be incorrect
```

So can custom protocols that are subtypes of `Iterable[object]` but supertypes of all
`GeneratorType` specializations. We do not suggest replacing these with `Collection` either, since
that could remove members such as `gi_running` in this example:

```py
from typing import Protocol, Iterator

class GeneratorLike(Protocol):
    def __iter__(self) -> Iterator[int]: ...
    @property
    def gi_running(self) -> bool: ...

def check_custom(x: GeneratorLike):
    if x:  # snapshot: truthiness-test-of-iterable
        print(x.gi_running)
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:39:8
   |
38 | def check_custom(x: GeneratorLike):
   |                     ------------- Inferred as `GeneratorLike` due to this annotation
39 |     if x:  # snapshot: truthiness-test-of-iterable
   |        ^ `GeneratorLike` object tested for truthiness
info: `GeneratorLike` objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
   |
38 | def check_custom(x: GeneratorLike):
   -     if x:  # snapshot: truthiness-test-of-iterable
39 +     if len(tuple(x)):  # snapshot: truthiness-test-of-iterable
40 |         print(x.gi_running)
   |
note: This is a display-only fix and is likely to be incorrect
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

## Annotated iterable attributes

An attribute's annotation can explain its inferred type, just as a parameter annotation can. In the
below example, we offer a suggestion that points to the annotated assignment that declares the
attribute:

```py
from collections.abc import Iterable

class Container:
    items: Iterable[int]

def check(container: Container):
    if container.items:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:7:8
  |
4 |     items: Iterable[int]
  |            ------------- Inferred as `Iterable[int]` due to this annotation
5 |
6 | def check(container: Container):
7 |     if container.items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^^^^^^^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[int]` as the annotation on line 4
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
  - from collections.abc import Iterable
1 + from collections.abc import Iterable, Collection
2 |
3 | class Container:
  -     items: Iterable[int]
4 +     items: Collection[int]
5 |
  |
note: This is an unsafe fix and may change runtime behavior
```

## Collection fixes preserve element annotations

Changing `Iterable` to `Collection` preserves the spelling of the element type. An imported type can
be available only under an alias, and the iterable itself can also be imported under an alias:

```py
from collections.abc import Iterable as I
from pathlib import Path as P

def check(items: I[P]):
    if items:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:5:8
  |
4 | def check(items: I[P]):
  |                  ---- Inferred as `Iterable[Path]` due to this annotation
5 |     if items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^ `Iterable[Path]` object tested for truthiness
info: `Iterable[Path]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[P]` as the annotation on line 4
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
  - from collections.abc import Iterable as I
1 + from collections.abc import Iterable as I, Collection
2 | from pathlib import Path as P
3 |
  - def check(items: I[P]):
4 + def check(items: Collection[P]):
5 |     if items:  # snapshot: truthiness-test-of-iterable
  |
note: This is an unsafe fix and may change runtime behavior
```

Qualified names are preserved too, so the fix does not require new imports for the element type:

```py
import datetime
import typing

def check_qualified(items: typing.Iterable[datetime.date]):
    if items:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:11:8
   |
10 | def check_qualified(items: typing.Iterable[datetime.date]):
   |                            ------------------------------ Inferred as `Iterable[date]` due to this annotation
11 |     if items:  # snapshot: truthiness-test-of-iterable
   |        ^^^^^ `Iterable[date]` object tested for truthiness
info: `Iterable[date]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[datetime.date]` as the annotation on line 10
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
   |
   - from collections.abc import Iterable as I
1  + from collections.abc import Iterable as I, Collection
2  | from pathlib import Path as P
--------------------------------------------------------------------------------
9  |
   - def check_qualified(items: typing.Iterable[datetime.date]):
10 + def check_qualified(items: Collection[datetime.date]):
11 |     if items:  # snapshot: truthiness-test-of-iterable
   |
note: This is an unsafe fix and may change runtime behavior
```

## Collection fixes preserve forward references

A quoted annotation remains quoted when its iterable type is replaced. Removing the quotes would
evaluate the reference to `Later` before the class is defined:

```toml
[environment]
python-version = "3.12"
```

```py
from collections.abc import Iterable

def check(items: "Iterable[Later]"):
    if items:  # snapshot: truthiness-test-of-iterable
        pass

class Later: ...
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:4:8
  |
3 | def check(items: "Iterable[Later]"):
  |                  ----------------- Inferred as `Iterable[Later]` due to this annotation
4 |     if items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^ `Iterable[Later]` object tested for truthiness
info: `Iterable[Later]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[Later]` as the annotation on line 3
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
  - from collections.abc import Iterable
1 + from collections.abc import Iterable, Collection
2 |
  - def check(items: "Iterable[Later]"):
3 + def check(items: "Collection[Later]"):
4 |     if items:  # snapshot: truthiness-test-of-iterable
  |
note: This is an unsafe fix and may change runtime behavior
```

The element annotation can also be quoted on its own. The fix preserves these quotes as well:

```py
def check_element(items: Iterable["Another"]):
    if items:  # snapshot: truthiness-test-of-iterable
        pass

class Another: ...
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:9:8
  |
8 | def check_element(items: Iterable["Another"]):
  |                          ------------------- Inferred as `Iterable[Another]` due to this annotation
9 |     if items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^ `Iterable[Another]` object tested for truthiness
info: `Iterable[Another]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection["Another"]` as the annotation on line 8
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
  - from collections.abc import Iterable
1 + from collections.abc import Iterable, Collection
2 |
--------------------------------------------------------------------------------
7 | class Later: ...
  - def check_element(items: Iterable["Another"]):
8 + def check_element(items: Collection["Another"]):
9 |     if items:  # snapshot: truthiness-test-of-iterable
  |
note: This is an unsafe fix and may change runtime behavior
```

## Collection fixes use the annotation's scope

An import that is visible where an attribute is tested can be shadowed in the class containing its
annotation. The fix uses a qualified name when the class shadows `Collection`:

```py
from collections.abc import Collection, Iterable

class Container:
    Collection = int
    items: Iterable[int]

def check(container: Container):
    if container.items:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:8:8
  |
5 |     items: Iterable[int]
  |            ------------- Inferred as `Iterable[int]` due to this annotation
6 |
7 | def check(container: Container):
8 |     if container.items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^^^^^^^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[int]` as the annotation on line 5
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
1 + import collections.abc
2 | from collections.abc import Collection, Iterable
3 |
4 | class Container:
5 |     Collection = int
  -     items: Iterable[int]
6 +     items: collections.abc.Collection[int]
7 |
  |
note: This is an unsafe fix and may change runtime behavior
```

A method's parameter annotations are also evaluated in the class scope, even though the condition is
evaluated in the method's scope:

```py
class Checker:
    Collection = int

    def check(self, items: Iterable[int]):
        if items:  # snapshot: truthiness-test-of-iterable
            pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:14:12
   |
13 |     def check(self, items: Iterable[int]):
   |                            ------------- Inferred as `Iterable[int]` due to this annotation
14 |         if items:  # snapshot: truthiness-test-of-iterable
   |            ^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[int]` as the annotation on line 13
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
   |
1  + import collections.abc
2  | from collections.abc import Collection, Iterable
--------------------------------------------------------------------------------
13 |
   -     def check(self, items: Iterable[int]):
14 +     def check(self, items: collections.abc.Collection[int]):
15 |         if items:  # snapshot: truthiness-test-of-iterable
   |
note: This is an unsafe fix and may change runtime behavior
```

## Collection suggestions for wrapped annotations

Replacing the outer name in an `Annotated` annotation would discard its metadata and produce an
invalid `Collection` specialization. For these annotations, we suggest reworking the annotation
without offering an annotation edit:

```py
from collections.abc import Iterable
from typing import Annotated

def check(items: Annotated[Iterable[int], "metadata"]):
    if items:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:5:8
  |
4 | def check(items: Annotated[Iterable[int], "metadata"]):
  |                  ------------------------------------ Inferred as `Iterable[int]` due to this annotation
5 |     if items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Consider reworking the annotation on line 4 to use `collections.abc.Collection`
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
4 | def check(items: Annotated[Iterable[int], "metadata"]):
  -     if items:  # snapshot: truthiness-test-of-iterable
5 +     if len(tuple(items)):  # snapshot: truthiness-test-of-iterable
6 |         pass
  |
note: This is a display-only fix and is likely to be incorrect
```

## Collection fixes before Python 3.9

Before Python 3.9, `collections.abc.Collection` cannot be subscripted at runtime. For an eagerly
evaluated annotation, the fix imports `Collection` from `typing`:

```toml
[environment]
python-version = "3.8"
```

```py
from typing import Iterable

def check(items: Iterable[int]):
    if items:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:4:8
  |
3 | def check(items: Iterable[int]):
  |                  ------------- Inferred as `Iterable[int]` due to this annotation
4 |     if items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Use `typing.Collection[int]` as the annotation on line 3
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
  - from typing import Iterable
1 + from typing import Iterable, Collection
2 |
  - def check(items: Iterable[int]):
3 + def check(items: Collection[int]):
4 |     if items:  # snapshot: truthiness-test-of-iterable
  |
note: This is an unsafe fix and may change runtime behavior
```

The fix also preserves legacy spellings of nested types. Using `list[int]` or `int | str` in an
eagerly evaluated annotation would fail at runtime on Python 3.8:

```py
from typing import List, Union

def check_nested(items: Iterable[List[int]]):
    if items:  # snapshot: truthiness-test-of-iterable
        pass

def check_union(items: Iterable[Union[int, str]]):
    if items:  # snapshot: truthiness-test-of-iterable
        pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:9:8
  |
8 | def check_nested(items: Iterable[List[int]]):
  |                         ------------------- Inferred as `Iterable[list[int]]` due to this annotation
9 |     if items:  # snapshot: truthiness-test-of-iterable
  |        ^^^^^ `Iterable[list[int]]` object tested for truthiness
info: `Iterable[list[int]]` objects can be generators, and generators are truthy even when empty
help: Use `typing.Collection[List[int]]` as the annotation on line 8
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
  - from typing import Iterable
1 + from typing import Iterable, Collection
2 |
--------------------------------------------------------------------------------
7 |
  - def check_nested(items: Iterable[List[int]]):
8 + def check_nested(items: Collection[List[int]]):
9 |     if items:  # snapshot: truthiness-test-of-iterable
  |
note: This is an unsafe fix and may change runtime behavior


warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
  --> src/mdtest_snippet.py:13:8
   |
12 | def check_union(items: Iterable[Union[int, str]]):
   |                        ------------------------- Inferred as `Iterable[int | str]` due to this annotation
13 |     if items:  # snapshot: truthiness-test-of-iterable
   |        ^^^^^ `Iterable[int | str]` object tested for truthiness
info: `Iterable[int | str]` objects can be generators, and generators are truthy even when empty
help: Use `typing.Collection[Union[int, str]]` as the annotation on line 12
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
   |
   - from typing import Iterable
1  + from typing import Iterable, Collection
2  |
--------------------------------------------------------------------------------
11 |
   - def check_union(items: Iterable[Union[int, str]]):
12 + def check_union(items: Collection[Union[int, str]]):
13 |     if items:  # snapshot: truthiness-test-of-iterable
   |
note: This is an unsafe fix and may change runtime behavior
```

## Iterable annotations in other first-party files

When an imported variable's annotation comes from another first-party file, the diagnostic points to
that annotation and includes its relative path and line number in the suggestion:

`items.py`:

```py
from collections.abc import Iterable

values: Iterable[int]
```

`main.py`:

```py
from items import values

if values:  # snapshot: truthiness-test-of-iterable
    pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/main.py:3:4
  |
3 | if values:  # snapshot: truthiness-test-of-iterable
  |    ^^^^^^ `Iterable[int]` object tested for truthiness
  |
 ::: src/items.py:3:9
  |
3 | values: Iterable[int]
  |         ------------- Inferred as `Iterable[int]` due to this annotation
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Consider using `collections.abc.Collection[int]` in the annotation on line 3 of src/items.py
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
2 |
  - if values:  # snapshot: truthiness-test-of-iterable
3 + if len(tuple(values)):  # snapshot: truthiness-test-of-iterable
4 |     pass
  |
note: This is a display-only fix and is likely to be incorrect
```

## Iterable annotations in dependencies

An annotation in a third-party library also explains the inferred type. The diagnostic suggests
changing the truthiness test, but does not ask the user to edit the library's annotation:

```toml
[environment]
python = "/.venv"
```

`/.venv/<path-to-site-packages>/items.pyi`:

```pyi
from collections.abc import Iterable

values: Iterable[int]
```

`main.py`:

```py
import items

if items.values:  # snapshot: truthiness-test-of-iterable
    pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/main.py:3:4
  |
3 | if items.values:  # snapshot: truthiness-test-of-iterable
  |    ^^^^^^^^^^^^ `Iterable[int]` object tested for truthiness
  |
 ::: .venv/<path-to-site-packages>/items.pyi:3:9
  |
3 | values: Iterable[int]
  |         ------------- Inferred as `Iterable[int]` due to this annotation
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
  |
2 |
  - if items.values:  # snapshot: truthiness-test-of-iterable
3 + if len(tuple(items.values)):  # snapshot: truthiness-test-of-iterable
4 |     pass
  |
note: This is a display-only fix and is likely to be incorrect
```

## Iterable values without variable annotations

A function call can produce an iterable without an annotated variable at the use site. We still
explain why the truthiness test is suspicious and suggest testing its length:

```py
from collections.abc import Iterable

def get_items() -> Iterable[int]:
    return []

if get_items():  # snapshot: truthiness-test-of-iterable
    pass
```

```snapshot
warning[truthiness-test-of-iterable]: Suspicious boolean test of an `Iterable`
 --> src/mdtest_snippet.py:6:4
  |
6 | if get_items():  # snapshot: truthiness-test-of-iterable
  |    ^^^^^^^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Test the length of the iterable instead of its truthiness
  |
5 |
  - if get_items():  # snapshot: truthiness-test-of-iterable
6 + if len(tuple(get_items())):  # snapshot: truthiness-test-of-iterable
7 |     pass
  |
note: This is a display-only fix and is likely to be incorrect
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
  |                 ^^^^^^^^^ `() -> bool` object tested for truthiness
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
3 | def check_nested(flag: bool, predicate: Callable[[], bool], items: Iterable[int]):
  |                                                                    ------------- Inferred as `Iterable[int]` due to this annotation
4 |     if flag and predicate:  # snapshot: truthiness-test-of-callable
5 |         pass
6 |     if flag and items:  # snapshot: truthiness-test-of-iterable
  |                 ^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[int]` as the annotation on line 3
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
  |
  - from collections.abc import Callable, Iterable
1 + from collections.abc import Callable, Iterable, Collection
2 |
  - def check_nested(flag: bool, predicate: Callable[[], bool], items: Iterable[int]):
3 + def check_nested(flag: bool, predicate: Callable[[], bool], items: Collection[int]):
4 |     if flag and predicate:  # snapshot: truthiness-test-of-callable
  |
note: This is an unsafe fix and may change runtime behavior
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
   |        ^^^^^ `() -> bool` object tested for truthiness
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
   |                 ^^^^^^ `() -> bool` object tested for truthiness
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
   |        ^^^^^^^^^ `() -> bool` object tested for truthiness
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
19 | def check_mixed(predicate: Callable[[], bool], items: Iterable[int]):
   |                                                       ------------- Inferred as `Iterable[int]` due to this annotation
20 |     # snapshot: truthiness-test-of-callable
21 |     # snapshot: truthiness-test-of-iterable
22 |     if predicate and items:
   |                      ^^^^^ `Iterable[int]` object tested for truthiness
info: `Iterable[int]` objects can be generators, and generators are truthy even when empty
help: Use `collections.abc.Collection[int]` as the annotation on line 19
info: A `Collection` must define `__len__`, so `Collection` excludes generators
help: Alternatively, test the length of the iterable instead of its truthiness
   |
   - from collections.abc import Callable, Iterable
1  + from collections.abc import Callable, Iterable, Collection
2  |
--------------------------------------------------------------------------------
18 |         pass
   - def check_mixed(predicate: Callable[[], bool], items: Iterable[int]):
19 + def check_mixed(predicate: Callable[[], bool], items: Collection[int]):
20 |     # snapshot: truthiness-test-of-callable
   |
note: This is an unsafe fix and may change runtime behavior
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
  |        ^^^^^^^^^ `() -> bool` object tested for truthiness
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

## Truthiness tests of unions with `None`

The `truthiness-test-of-none-union` opt-in rule detects truthiness checks that may accidentally
conflate `None` with other falsy values.

### Basic idea

Consider the following problematic `take` function where a limit of `0` would be treated as if it
were `None`, which is probably not the intended behavior:

```py
def take(items: list[str], limit: int | None = None) -> list[str]:
    # snapshot: truthiness-test-of-none-union
    if not limit:
        return items
    return items[:limit]
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `int | None` does not distinguish `None` from other falsy values
 --> src/mdtest_snippet.py:3:12
  |
3 |     if not limit:
  |            ^^^^^ `None` and `0` are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

Explicit `bool()` calls indicate an intentional truthiness test:

```py
def take_explicit(items: list[str], limit: int | None = None) -> list[str]:
    if not bool(limit):  # no diagnostic
        return items
    return items[:limit]
```

### Covered types

This rule triggers on unions with `None` and other types that have falsy values:

```py
from typing import Any, Literal

def check(flag: bool | None):
    if flag:  # error: [truthiness-test-of-none-union]
        pass
    if bool(flag):  # no diagnostic
        pass

def check(integer: int | None):
    if integer:  # error: [truthiness-test-of-none-union]
        pass
    if bool(integer):  # no diagnostic
        pass

def check(text: str | None):
    if text:  # error: [truthiness-test-of-none-union]
        pass
    if bool(text):  # no diagnostic
        pass

def check(data: bytes | None):
    if data:  # error: [truthiness-test-of-none-union]
        pass
    if bool(data):  # no diagnostic
        pass

def check(number: float | None):
    if number:  # error: [truthiness-test-of-none-union]
        pass
    if bool(number):  # no diagnostic
        pass

def check(number: complex | None):
    if number:  # error: [truthiness-test-of-none-union]
        pass
    if bool(number):  # no diagnostic
        pass

def check(items: list[int] | None):
    if items:  # error: [truthiness-test-of-none-union]
        pass
    if bool(items):  # no diagnostic
        pass

def check(items: list[Any] | None):
    if items:  # error: [truthiness-test-of-none-union]
        pass
    if bool(items):  # no diagnostic
        pass

def check(mapping: dict[str, int] | None):
    if mapping:  # error: [truthiness-test-of-none-union]
        pass
    if bool(mapping):  # no diagnostic
        pass
```

It also triggers if multiple other types could be falsy:

```py
def check(value: int | str | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
    if bool(value):  # no diagnostic
        pass
```

It also triggers if there are (additional) types in the union that are always truthy:

```py
def check(value: str | Literal[True] | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
    if bool(value):  # no diagnostic
        pass
```

The rule does *not* trigger on unions with `None` where the other types are always truthy, such as
`Match[str]`:

```py
import re

def re_match_is_always_truthy(match: re.Match[str]):
    reveal_type(bool(match))  # revealed: Literal[True]

def check(match: re.Match[str] | None):
    if match:  # no diagnostic
        pass
    if bool(match):  # no diagnostic
        pass
```

The rule does *not* trigger on unions with `None` whose other elements are all dynamic types. A
dynamic type can materialize to an always-truthy type, so to respect the gradual guarantee, the rule
does not trigger here:

```py
def check(value: Any | None):
    if value:  # no diagnostic
        pass
    if bool(value):  # no diagnostic
        pass
```

The rule does trigger, however, if the union includes dynamic types in addition to two types that
can be falsy:

```py
def check(value: int | Any | None):
    # snapshot: truthiness-test-of-none-union
    if value:
        pass
    if bool(value):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `int | Any | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:83:8
   |
83 |     if value:
   |        ^^^^^ Both `None` and non-`None` values can be falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

A custom class without any specific evidence that it could be falsy is currently excluded from this
rule. In principle, a subclass could introduce falsy behavior, but considering this possibility
leads to many false positives in the ecosystem, since most classes are not final. So the rule does
not apply here:

```py
from typing import final

class Custom: ...

def check(value: Custom | None):
    if value:  # no diagnostic
        pass
```

However, if the class (or a base class) defines a `__bool__` or `__len__` method, we consider it
potentially falsy and do apply the rule:

```py
class AmbiguousTruthiness:
    def __bool__(self) -> bool:
        raise NotImplementedError

def check(value: AmbiguousTruthiness | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
    if bool(value):  # no diagnostic
        pass

class AmbiguousLength:
    def __len__(self) -> int:
        raise NotImplementedError

def check(value: AmbiguousLength | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
    if bool(value):  # no diagnostic
        pass

class ChildOfAmbiguousTruthiness(AmbiguousTruthiness):
    pass

def check(value: ChildOfAmbiguousTruthiness | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
    if bool(value):  # no diagnostic
        pass

class AlwaysTruthy:
    def __bool__(self) -> Literal[True]:
        return True

def check(value: AlwaysTruthy | None):
    if value:  # no diagnostic
        pass

class AlwaysFalsy:
    def __bool__(self) -> Literal[False]:
        return False

# Also add `AlwaysTruthy` to the union or otherwise this would trigger `redundant-condition`
def check(value: AlwaysFalsy | AlwaysTruthy | None):
    if value:  # error: [truthiness-test-of-none-union]
        pass
    if bool(value):  # no diagnostic
        pass
```

### Type-specific explanations

For common builtin types, the annotation identifies the falsy value that can be confused with `None`
to help users identify the problem.

Note: for a type like `int | None`, the falsy values that could be confused are not just `None` and
`0`. `False` and instances of custom subclasses of `int` could also be falsy, but it would be too
verbose to mention that in the diagnostic hint, so we just list `None` and `0` here:

```py
def check(integer: int | None):
    # snapshot: truthiness-test-of-none-union
    if integer:
        pass
    if bool(integer):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `int | None` does not distinguish `None` from other falsy values
 --> src/mdtest_snippet.py:3:8
  |
3 |     if integer:
  |        ^^^^^^^ `None` and `0` are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(flag: bool | None):
    # snapshot: truthiness-test-of-none-union
    if flag:
        pass
    if bool(flag):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `bool | None` does not distinguish `None` from other falsy values
 --> src/mdtest_snippet.py:9:8
  |
9 |     if flag:
  |        ^^^^ `None` and `False` are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(text: str | None):
    # snapshot: truthiness-test-of-none-union
    if text:
        pass
    if bool(text):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `str | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:15:8
   |
15 |     if text:
   |        ^^^^ `None` and the empty string are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(data: bytes | None):
    # snapshot: truthiness-test-of-none-union
    if data:
        pass
    if bool(data):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `bytes | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:21:8
   |
21 |     if data:
   |        ^^^^ `None` and an empty bytestring are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(number: float | None):
    # snapshot: truthiness-test-of-none-union
    if number:
        pass
    if bool(number):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `float | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:27:8
   |
27 |     if number:
   |        ^^^^^^ `None` and `0` are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(items: list[int] | None):
    # snapshot: truthiness-test-of-none-union
    if items:
        pass
    if bool(items):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `list[int] | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:33:8
   |
33 |     if items:
   |        ^^^^^ `None` and an empty list are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
def check(mapping: dict[str, int] | None):
    # snapshot: truthiness-test-of-none-union
    if mapping:
        pass
    if bool(mapping):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `dict[str, int] | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:39:8
   |
39 |     if mapping:
   |        ^^^^^^^ `None` and an empty dictionary are both falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

Other unions with `None` fall back to a general explanation:

```py
def check(value: str | int | None):
    # snapshot: truthiness-test-of-none-union
    if value:
        pass
    if bool(value):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `str | int | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:45:8
   |
45 |     if value:
   |        ^^^^^ Both `None` and non-`None` values can be falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

```py
class Custom:
    def __bool__(self) -> bool:
        return False

def check(value: Custom | None):
    # snapshot: truthiness-test-of-none-union
    if value:
        pass
    if bool(value):  # no diagnostic
        pass
```

```snapshot
info[truthiness-test-of-none-union]: Boolean test on `Custom | None` does not distinguish `None` from other falsy values
  --> src/mdtest_snippet.py:55:8
   |
55 |     if value:
   |        ^^^^^ Both `None` and non-`None` values can be falsy
help: Use `is None` or `is not None` to check for presence of the value
help: Use `bool(...)` if testing truthiness is intentional
```

### Boolean contexts

The rule triggers in all of these Boolean contexts:

```py
def check(limit: int | None, items: list[int | None]):
    if limit:  # error: [truthiness-test-of-none-union]
        pass
    if bool(limit):  # no diagnostic
        pass
    elif limit:  # error: [truthiness-test-of-none-union]
        pass

    def _():
        while limit:  # error: [truthiness-test-of-none-union]
            break

    result = 1 if limit else 0  # error: [truthiness-test-of-none-union]

    result = not limit  # error: [truthiness-test-of-none-union]

    filtered = [x for x in items if x]  # error: [truthiness-test-of-none-union]

    match limit:
        case _ if limit:  # error: [truthiness-test-of-none-union]
            pass

    def inner():
        assert limit  # error: [truthiness-test-of-none-union]

    if result := limit:  # error: [truthiness-test-of-none-union]
        pass

    if not (result := limit):  # error: [truthiness-test-of-none-union]
        pass
```

### Boolean expressions

Boolean operators used to compute values are exempt.

```py
def check(items: list[int] | None, flag: bool, other: bool):
    value = flag and items
    value = flag or items
    value = items or []
    value = items and flag
```

When the whole expression is used as a condition, each operand is checked:

```py
    if items and flag:  # error: [truthiness-test-of-none-union]
        pass
    if flag and items:  # error: [truthiness-test-of-none-union]
        pass
    if not (items or flag):  # error: [truthiness-test-of-none-union]
        pass
    if flag and (other or items):  # error: [truthiness-test-of-none-union]
        pass
```

### Unions formed by compound conditions

Combining individually unambiguous tests must not introduce a diagnostic merely because their
results include both `None` and other falsy values. Assignment expressions preserve this behavior.

```py
from typing import final

@final
class Match: ...

def check(enabled: bool, match: Match | None, title=None):
    if enabled and match:  # no diagnostic
        pass
    if match or enabled:  # no diagnostic
        pass
    if not (enabled and match):  # no diagnostic
        pass
    if result := enabled and match:  # no diagnostic
        pass
    if result := (inner := match or enabled):  # no diagnostic
        pass
    if enabled and title:  # no diagnostic
        pass
    if result := enabled and title:  # no diagnostic
        pass
    if result := (match if enabled else False):  # no diagnostic
        pass
```

Operands that can themselves conflate `None` with another falsy value still trigger the rule,
including in compound expressions wrapped in assignments.

```py
def check(enabled: bool, value: int | None):
    if enabled and value:  # error: [truthiness-test-of-none-union]
        pass
    if value or enabled:  # error: [truthiness-test-of-none-union]
        pass
    if result := enabled and value:  # error: [truthiness-test-of-none-union]
        pass
    if result := (inner := enabled or value):  # error: [truthiness-test-of-none-union]
        pass
    if result := (value if enabled else False):  # error: [truthiness-test-of-none-union]
        pass
```
