# Detection of boolean tests that are always truthy or always falsy

A common error in Python is to accidentally test truthiness of the wrong object: for example
`if func:` (which is always true) where `if func():` was intended, or `if coroutine():` where
`if await coroutine():` was intended. By default, ty alerts the user to these errors with the error
code `redundant-condition`, but only if the inferred type of the object is not assignable to `int`
and has fixed truthiness. This heuristic catches the `if func` and `if coroutine()` cases, while
avoiding false positives on cases such as `if DEBUG:` where `DEBUG = 0` or `DEBUG = False` is a
constant.

The remaining cases -- where the inferred type is assignable to `int`, or only short-circuit
evaluation makes the condition's truthiness fixed -- are covered by a separate, stricter rule
(`redundant-condition-strict`).

```toml
[environment]
python-version = "3.14"
python-platform = "linux"

[rules]
redundant-condition-strict = "error"
```

## Basic cases

We catch testing a function without calling it:

```py
def func(): ...

if func:  # snapshot: redundant-condition
    pass
```

```snapshot
warning[redundant-condition]: Function `func` is always truthy
 --> src/mdtest_snippet.py:3:4
  |
3 | if func:  # snapshot: redundant-condition
  |    ^^^^ Did you mean to call this function?
  |
2 |
  - if func:  # snapshot: redundant-condition
3 + if func():  # snapshot: redundant-condition
4 |     pass
  |
note: This is an unsafe fix and may change runtime behavior
```

And testing a method without calling it:

```py
class Foo:
    def bar(self) -> bool:
        return True

    def baz(self):
        if self.bar:  # snapshot: redundant-condition
            pass
```

```snapshot
warning[redundant-condition]: Method `Foo.bar` is always truthy
  --> src/mdtest_snippet.py:10:12
   |
10 |         if self.bar:  # snapshot: redundant-condition
   |            ^^^^^^^^ Did you mean to call this method?
   |
9  |     def baz(self):
   -         if self.bar:  # snapshot: redundant-condition
10 +         if self.bar():  # snapshot: redundant-condition
11 |             pass
   |
note: This is an unsafe fix and may change runtime behavior
```

And testing a generator expression without executing it:

```py
def work(items: list[int]):
    filtered = (item for item in items if item < 42)
    if filtered:  # snapshot: redundant-condition
        pass
    assert filtered  # error: [redundant-condition] "Object of type `GeneratorType[int, None, None]` is always truthy"
```

```snapshot
warning[redundant-condition]: A generator is always truthy
  --> src/mdtest_snippet.py:14:8
   |
14 |     if filtered:  # snapshot: redundant-condition
   |        ^^^^^^^^ Inferred type is `GeneratorType[int, None, None]`
help: Did you mean to use `any()`?
   |
13 |     filtered = (item for item in items if item < 42)
   -     if filtered:  # snapshot: redundant-condition
14 +     if any(filtered):  # snapshot: redundant-condition
15 |         pass
   |
note: This is a display-only fix and is likely to be incorrect
```

And testing an awaitable without awaiting it:

```py
async def coroutine(): ...
async def main():
    if coroutine():  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:19:8
   |
19 |     if coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
18 | async def main():
   -     if coroutine():  # snapshot: redundant-condition
19 +     if await coroutine():  # snapshot: redundant-condition
20 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

And testing a tuple that is known to always be empty or non-empty:

```py
class Foo:
    def __init__(self):
        self.two_element_tuple: tuple[int, int] = (423, 432)
        self.at_least_one_element: tuple[int, *tuple[int, ...]] = (42,)
        self.at_least_two_elements: tuple[int, int, *tuple[int, ...]] = (42, 42)
        self.no_elements: tuple[()] = ()

    def other_method(self):
        if self.two_element_tuple:  # snapshot: redundant-condition
            pass
        if self.at_least_one_element:  # snapshot: redundant-condition
            pass
        if self.at_least_two_elements:  # snapshot: redundant-condition
            pass
        if self.no_elements:  # snapshot: redundant-condition
            pass

        # error: [redundant-condition] "Object of type `tuple[int, *tuple[int, ...]]` is always truthy"
        assert self.at_least_one_element
        # error: [redundant-condition] "Object of type `tuple[int, int, *tuple[int, ...]]` is always truthy"
        assert self.at_least_two_elements
```

```snapshot
warning[redundant-condition]: A 2-element tuple is always truthy
  --> src/mdtest_snippet.py:29:12
   |
29 |         if self.two_element_tuple:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `tuple[int, int]`


warning[redundant-condition]: A tuple with >=1 element is always truthy
  --> src/mdtest_snippet.py:31:12
   |
31 |         if self.at_least_one_element:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `tuple[int, *tuple[int, ...]]`


warning[redundant-condition]: A tuple with >=2 elements is always truthy
  --> src/mdtest_snippet.py:33:12
   |
33 |         if self.at_least_two_elements:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `tuple[int, int, *tuple[int, ...]]`


warning[redundant-condition]: An empty tuple is always falsy
  --> src/mdtest_snippet.py:35:12
   |
35 |         if self.no_elements:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^ Inferred type is `tuple[()]`
```

Annotating a variable as `tuple[X]` is almost always a mistake (the user almost always meant to
write `tuple[X, ...]`), so we point to the annotation and suggest an arbitrary-length tuple instead:

```py
class Bar:
    def __init__(self):
        self.single_element_tuple: tuple[int] = (42,)

    def first_method(self):
        self.single_element_tuple = (56,)

    def other_method(self, y: tuple[str]):
        if self.single_element_tuple:  # snapshot: redundant-condition
            pass

        if y:  # snapshot: redundant-condition
            pass
```

```snapshot
warning[redundant-condition]: A 1-element tuple is always truthy
  --> src/mdtest_snippet.py:50:12
   |
50 |         if self.single_element_tuple:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `tuple[int]`
   |
  ::: src/mdtest_snippet.py:44:36
   |
44 |         self.single_element_tuple: tuple[int] = (42,)
   |                                    ----------
   |                                    |
   |                                    Inferred as a 1-element tuple due to this annotation
   |                                    Did you mean `tuple[int, ...]`?


warning[redundant-condition]: A 1-element tuple is always truthy
  --> src/mdtest_snippet.py:53:12
   |
49 |     def other_method(self, y: tuple[str]):
   |                               ----------
   |                               |
   |                               Inferred as a 1-element tuple due to this annotation
   |                               Did you mean `tuple[str, ...]`?
50 |         if self.single_element_tuple:  # snapshot: redundant-condition
51 |             pass
52 |
53 |         if y:  # snapshot: redundant-condition
   |            ^ Inferred type is `tuple[str]`
```

And testing `None`:

```py
X = None

if X:  # snapshot: redundant-condition
    pass
```

```snapshot
warning[redundant-condition]: `None` is always falsy
  --> src/mdtest_snippet.py:57:4
   |
57 | if X:  # snapshot: redundant-condition
   |    ^
```

And testing a string that is known to always be truthy or always be falsy:

```py
x = "foo"
y = ""

if x:  # snapshot: redundant-condition
    pass

if y:  # snapshot: redundant-condition
    pass
```

```snapshot
warning[redundant-condition]: A nonempty string is always truthy
  --> src/mdtest_snippet.py:62:4
   |
62 | if x:  # snapshot: redundant-condition
   |    ^ Inferred type is `Literal["foo"]`


warning[redundant-condition]: An empty string is always falsy
  --> src/mdtest_snippet.py:65:4
   |
65 | if y:  # snapshot: redundant-condition
   |    ^ Inferred type is `Literal[""]`
```

or even a union of strings that is known to always be truthy:

```py
from typing import Literal

def f(x: Literal["a", "b"]):
    if x:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: A nonempty string is always truthy
  --> src/mdtest_snippet.py:70:8
   |
70 |     if x:  # snapshot: redundant-condition
   |        ^ Inferred type is `Literal["a", "b"]`
```

and testing a `TypedDict` that is known to always be truthy:

```py
from typing import TypedDict, NotRequired, Required

class NeverEmpty(TypedDict):
    x: int
    y: str

class AlsoNeverEmpty(TypedDict, total=False):
    x: Required[int]

class SometimesEmpty(TypedDict):
    x: NotRequired[int]

class AlsoSometimesEmpty(TypedDict, total=False):
    x: int

def test(
    never_empty: NeverEmpty,
    also_never_empty: AlsoNeverEmpty,
    sometimes_empty: SometimesEmpty,
    also_sometimes_empty: AlsoSometimesEmpty,
):
    if never_empty:  # snapshot: redundant-condition
        pass

    if also_never_empty:  # snapshot: redundant-condition
        pass

    if sometimes_empty:  # no diagnostic
        pass

    if also_sometimes_empty:  # no diagnostic
        pass

    assert never_empty  # error: [redundant-condition] "TypedDict `NeverEmpty` with 2 required fields is always truthy"
    assert also_never_empty  # error: [redundant-condition] "TypedDict `AlsoNeverEmpty` with 1 required field is always truthy"
    assert sometimes_empty  # no diagnostic
    assert also_sometimes_empty  # no diagnostic
```

```snapshot
warning[redundant-condition]: A TypedDict with 2 required fields is always truthy
  --> src/mdtest_snippet.py:93:8
   |
93 |     if never_empty:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^ Inferred type is `NeverEmpty`
   |
  ::: src/mdtest_snippet.py:74:7
   |
74 | class NeverEmpty(TypedDict):
   |       ---------- `NeverEmpty` defined here
75 |     x: int
   |     ------ First required field defined here


warning[redundant-condition]: A TypedDict with 1 required field is always truthy
  --> src/mdtest_snippet.py:96:8
   |
96 |     if also_never_empty:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^^^ Inferred type is `AlsoNeverEmpty`
   |
  ::: src/mdtest_snippet.py:78:7
   |
78 | class AlsoNeverEmpty(TypedDict, total=False):
   |       -------------- `AlsoNeverEmpty` defined here
79 |     x: Required[int]
   |     ---------------- Required field declared here
```

and testing an object that is known to be always truthy due to it being `@final` and not defining
`__bool__` or `__len__`:

```py
from re import Pattern

def f(x: Pattern[str]):
    if x:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:112:8
    |
112 |     if x:  # snapshot: redundant-condition
    |        ^ Inferred type is `Pattern[str]`
info: `Pattern` instances are always truthy because `Pattern` cannot be subclassed and does not define `__bool__` or `__len__`
   --> stdlib/re.pyi:285:1
    |
285 | / @final
286 | | class Pattern(Generic[AnyStr]):
    | |______________________________^ `Pattern` defined here
```

## Enum instances

An enum with members is implicitly final, so its instances are always truthy if the enum defines
neither `__bool__` nor `__len__`.

```py
from enum import Enum

class Choice(Enum):
    FIRST = 1
    SECOND = 2

def f(choice: Choice):
    if choice:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
 --> src/mdtest_snippet.py:8:8
  |
8 |     if choice:  # snapshot: redundant-condition
  |        ^^^^^^ Inferred type is `Choice`
info: `Choice` instances are always truthy because `Choice` cannot be subclassed and does not define `__bool__` or `__len__`
 --> src/mdtest_snippet.py:3:7
  |
3 | class Choice(Enum):
  |       ^^^^^^^^^^^^ `Choice` defined here
```

## Inherited required `TypedDict` fields

A required field makes a `TypedDict` nonempty even when the field is inherited from a class in
another module. The diagnostic points to the inherited field's declaration in that module.

`base.py`:

```py
from typing import TypedDict

class Base(TypedDict):
    value: int
```

`child.py`:

```py
from base import Base

class Child(Base):
    pass

def check(value: Child):
    if value:  # snapshot: redundant-condition
        pass
    assert value  # error: [redundant-condition] "TypedDict `Child` with 1 required field is always truthy"
```

```snapshot
warning[redundant-condition]: A TypedDict with 1 required field is always truthy
 --> src/child.py:7:8
  |
7 |     if value:  # snapshot: redundant-condition
  |        ^^^^^ Inferred type is `Child`
  |
 ::: src/child.py:3:7
  |
3 | class Child(Base):
  |       ----- `Child` defined here
  |
 ::: src/base.py:4:5
  |
4 |     value: int
  |     ---------- Required field declared here
```

## Required keys established by narrowing

A key-presence check can narrow an open `TypedDict` to an unnamed schema with required keys. The
diagnostic describes the number of required fields without inventing a class name.

```py
from typing import TypedDict

class Record(TypedDict):
    pass

def check(value: Record):
    if "x" in value:
        if value:  # error: [redundant-condition] "A TypedDict with 1 required field is always truthy"
            pass
```

## One-element tuple annotation hints

A named tuple or another tuple subclass can deliberately have exactly one element. An annotation
naming that class is not a mistaken use of `tuple[T]`, so we report its truthiness without
suggesting an arbitrary-length tuple annotation.

```py
from typing import NamedTuple

class Record(NamedTuple):
    value: int

class SingleTuple(tuple[int]):
    pass

def check(record: Record, single: SingleTuple):
    if record:  # snapshot: redundant-condition
        print(record.value)
    if single:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: A 1-element tuple is always truthy
  --> src/mdtest_snippet.py:10:8
   |
10 |     if record:  # snapshot: redundant-condition
   |        ^^^^^^ Inferred type is `Record`


warning[redundant-condition]: A 1-element tuple is always truthy
  --> src/mdtest_snippet.py:12:8
   |
12 |     if single:  # snapshot: redundant-condition
   |        ^^^^^^ Inferred type is `SingleTuple`
```

An implicit type alias for `tuple[T]` still refers to the built-in tuple type, so it remains
eligible for the annotation hint.

```py
IntTuple = tuple[int]

def check_alias(value: IntTuple):
    if value:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: A 1-element tuple is always truthy
  --> src/mdtest_snippet.py:17:8
   |
16 | def check_alias(value: IntTuple):
   |                        --------
   |                        |
   |                        Inferred as a 1-element tuple due to this annotation
   |                        Did you mean `tuple[int, ...]`?
17 |     if value:  # snapshot: redundant-condition
   |        ^^^^^ Inferred type is `tuple[int]`
```

The diagnostic still explains the one-element annotation when the suggested replacement would
contain notation that cannot be used in a Python annotation, such as a type variable's scope suffix.
In these cases, we omit the replacement suggestion, including when the type variable is nested
inside another generic type.

```py
def check_generic[T](value: tuple[T]):
    if value:  # snapshot: redundant-condition
        pass

def check_nested_generic[T](value: tuple[list[T]]):
    if value:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: A 1-element tuple is always truthy
  --> src/mdtest_snippet.py:20:8
   |
19 | def check_generic[T](value: tuple[T]):
   |                             -------- Inferred as a 1-element tuple due to this annotation
20 |     if value:  # snapshot: redundant-condition
   |        ^^^^^ Inferred type is `tuple[T@check_generic]`


warning[redundant-condition]: A 1-element tuple is always truthy
  --> src/mdtest_snippet.py:24:8
   |
23 | def check_nested_generic[T](value: tuple[list[T]]):
   |                                    -------------- Inferred as a 1-element tuple due to this annotation
24 |     if value:  # snapshot: redundant-condition
   |        ^^^^^ Inferred type is `tuple[list[T@check_nested_generic]]`
```

## Tuple annotations in dependencies

A one-element tuple annotation in a dependency also explains why the condition is redundant. The
suggestion refers to the dependency's author, since the annotation is outside first-party code.

```toml
[environment]
python = "/.venv"
```

`/.venv/<path-to-site-packages>/records.pyi`:

```pyi
values: tuple[str]
```

`main.py`:

```py
import records

if records.values:  # snapshot: redundant-condition
    pass
```

```snapshot
warning[redundant-condition]: A 1-element tuple is always truthy
 --> src/main.py:3:4
  |
3 | if records.values:  # snapshot: redundant-condition
  |    ^^^^^^^^^^^^^^ Inferred type is `tuple[str]`
  |
 ::: .venv/<path-to-site-packages>/records.pyi:1:9
  |
1 | values: tuple[str]
  |         ----------
  |         |
  |         Inferred as a 1-element tuple due to this annotation
  |         The author of this code might have meant `tuple[str, ...]`?
```

## Other boolean contexts

Redundant conditions are not merely detected in `if`-statement tests. They are also detected in
unary `not` operations, `while` loops, `assert` statements, `if` expressions, `match` guards, and
comprehension `if` tests. When an `and` or `or` expression is used as a condition, each operand is
checked.

```py
def coinflip() -> bool:
    return True

def func(): ...

if not func:  # error: [redundant-condition]
    pass

if not not func:  # error: [redundant-condition]
    pass

a = True if func else False  # error: [redundant-condition]

if coinflip() if func else False:  # error: [redundant-condition]
    pass

b = func and coinflip()

if func and coinflip():  # error: [redundant-condition]
    pass

c = func or coinflip()

if func or coinflip():  # error: [redundant-condition]
    pass

[x for x in range(3) if func]  # error: [redundant-condition]

def function(flag: bool):
    if flag:
        pass
    elif func:  # error: [redundant-condition]
        pass

def _():
    assert func  # error: [redundant-condition]

def _():
    while func and coinflip():  # error: [redundant-condition]
        pass

def _():
    while not (func and coinflip()):  # error: [redundant-condition]
        pass

def f(x: str | int):
    match x:
        case str() if func:  # error: [redundant-condition]
            pass

def _():
    while func:  # error: [redundant-condition]
        pass
```

## Always truthy values appearing later in compound conditions

A subexpression in a compound condition can be inferred as always truthy or always falsy even if the
condition overall is inferred as having ambiguous truthiness. We still report these subexpressions:

```py
def func(): ...
def compound_statement_conditions(flag: bool, other: bool):
    if flag and func:  # snapshot: redundant-condition
        pass

    if other:
        pass
    elif flag and func:  # error: [redundant-condition]
        pass

    while flag and func:  # error: [redundant-condition]
        break

    match flag:
        case bool() if flag and func:  # error: [redundant-condition]
            pass

def compound_expression_conditions(flag: bool):
    selected = True if flag and func else False  # snapshot: redundant-condition
    filtered = [value for value in range(1) if flag and func]  # error: [redundant-condition]
    result = flag and func

def compound_assertion_condition(flag: bool):
    assert flag and func  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Function `func` is always truthy
 --> src/mdtest_snippet.py:3:17
  |
3 |     if flag and func:  # snapshot: redundant-condition
  |                 ^^^^ Did you mean to call this function?
  |
2 | def compound_statement_conditions(flag: bool, other: bool):
  -     if flag and func:  # snapshot: redundant-condition
3 +     if flag and func():  # snapshot: redundant-condition
4 |         pass
  |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Function `func` is always truthy
  --> src/mdtest_snippet.py:19:33
   |
19 |     selected = True if flag and func else False  # snapshot: redundant-condition
   |                                 ^^^^ Did you mean to call this function?
   |
18 | def compound_expression_conditions(flag: bool):
   -     selected = True if flag and func else False  # snapshot: redundant-condition
19 +     selected = True if flag and func() else False  # snapshot: redundant-condition
20 |     filtered = [value for value in range(1) if flag and func]  # error: [redundant-condition]
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Function `func` is always truthy
  --> src/mdtest_snippet.py:24:21
   |
24 |     assert flag and func  # snapshot: redundant-condition
   |                     ^^^^ Did you mean to call this function?
   |
23 | def compound_assertion_condition(flag: bool):
   -     assert flag and func  # snapshot: redundant-condition
24 +     assert flag and func()  # snapshot: redundant-condition
   |
note: This is an unsafe fix and may change runtime behavior
```

## Chained comparison conditions

A comparison chain used directly as a condition is always false if any comparison is always false,
even when an earlier comparison returns an object with mutable truthiness. The assertion below
always fails because `1 < 0` is false.

```py
class Comparable:
    def __lt__(self, other: int) -> object:
        return object()

def direct_condition(value: Comparable):
    assert value < 1 < 0  # error: [redundant-condition-strict] "Condition `value < 1 < 0` is always false"

def negated_condition(value: Comparable):
    if not (value < 1 < 0):  # error: [redundant-condition-strict] "Condition `not (value < 1 < 0)` is always true"
        pass
```

The defensive-exit exemption also applies when short-circuit evaluation makes a condition always
false.

```py
def defensive_condition(value: Comparable):
    if value < 1 < 0:
        raise ValueError
```

Saving the chain's result, or negating it outside a condition, can cause an intermediate object's
truthiness to be tested twice. Its truthiness can change between those tests, so neither test below
has fixed truthiness.

```py
def saved_condition(value: Comparable):
    saved = value < 1 < 0
    if saved:
        pass
    return not (value < 1 < 0)
```

## Conditional expressions used as conditions

When a conditional expression is tested for truthiness, its selected branch is also tested. We
report uncalled functions in either branch, even when the complete condition has ambiguous
truthiness. Reporting a branch also suppresses a redundant diagnostic on the complete condition.

```py
def ready() -> bool:
    return False

def uncalled_functions(flag: bool):
    if ready if flag else False:  # error: [redundant-condition] "Function `ready` is always truthy"
        pass
    if False if flag else ready:  # error: [redundant-condition] "Function `ready` is always truthy"
        pass
    if ready if flag else True:  # error: [redundant-condition] "Function `ready` is always truthy"
        pass
    assert ready if flag else False  # error: [redundant-condition] "Function `ready` is always truthy"
```

Negating a conditional expression also tests the selected branch's truthiness, even outside a
statement condition.

```py
def negated_selection(flag: bool) -> bool:
    return not (ready if flag else False)  # error: [redundant-condition] "Function `ready` is always truthy"
```

Selecting a function as a value does not test its truthiness, even inside a larger condition. The
`callable()` call below tests whether the selected value can be called:

```py
def selected_value(flag: bool):
    if callable(ready if flag else None):
        pass
```

Boolean branches retain the exemptions for compound-condition operands and defensive assertions. An
always-true conditional expression is reported as a whole outside an assertion:

```py
def boolean_branches(value: int, flag: bool):
    assert isinstance(value, int) if flag else True
    # error: [redundant-condition-strict] "Condition `isinstance(value, int) if flag else True` is always true"
    if isinstance(value, int) if flag else True:
        pass
```

Both branches of this conditional expression are truthy when evaluated directly as conditions. Even
if `value` has mutable truthiness, `value or True` short-circuits directly to the loop body when
`value` is truthy and evaluates `True` otherwise.

```py
def conditional_expression(value: object, flag: bool):
    # error: [redundant-condition-strict] "Condition `True if flag else (value or True)` is always true"
    while True if flag else (value or True):
        break
```

## Edge cases

A nonempty tuple subclass can still be falsy if it overrides `__bool__`:

```py
from typing import Any, Literal, Never
from types import CoroutineType

async def coroutine(): ...

class FalsyTuple(tuple[int, int]):
    def __bool__(self) -> Literal[False]:
        return False

def check_falsy_tuple(value: FalsyTuple):
    if value:  # error: [redundant-condition] "Object of type `FalsyTuple` is always falsy"
        pass
```

Simply calling an asynchronous function would not resolve the redundant condition: the function must
be called *and* awaited, so this is what the autofix suggests:

```py
async def inspect_async_function():
    if coroutine:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Function `coroutine` is always truthy
  --> src/mdtest_snippet.py:14:8
   |
14 |     if coroutine:  # snapshot: redundant-condition
   |        ^^^^^^^^^ Did you mean to `await` and call this function?
   |
13 | async def inspect_async_function():
   -     if coroutine:  # snapshot: redundant-condition
14 +     if await coroutine():  # snapshot: redundant-condition
15 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

Calling a function with an always-truthy return value does not resolve the redundant condition --
but they still probably meant to call the function, so we still offer autofixes in these cases:

```py
def always_truthy() -> Literal[True]:
    return True

def inspect_truthy_function():
    if always_truthy:  # snapshot: redundant-condition
        pass

async def always_truthy_coro() -> Literal[True]:
    return True

async def foo():
    if always_truthy_coro:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Function `always_truthy` is always truthy
  --> src/mdtest_snippet.py:20:8
   |
20 |     if always_truthy:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^ Did you mean to call this function?
   |
19 | def inspect_truthy_function():
   -     if always_truthy:  # snapshot: redundant-condition
20 +     if always_truthy():  # snapshot: redundant-condition
21 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Function `always_truthy_coro` is always truthy
  --> src/mdtest_snippet.py:27:8
   |
27 |     if always_truthy_coro:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^^^^^ Did you mean to `await` and call this function?
   |
26 | async def foo():
   -     if always_truthy_coro:  # snapshot: redundant-condition
27 +     if await always_truthy_coro():  # snapshot: redundant-condition
28 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

If a function has parameters, we still offer a "fix", but we do not attempt to make the fix valid --
it's just to show the user visually what kind of edit we're suggesting that they make. The fix is
"display-only" to indicate that it's almost certainly incorrect:

```py
def wut(x): ...

if wut:  # snapshot: redundant-condition
    pass

async def wuttt(x): ...
async def bar():
    if wuttt:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Function `wut` is always truthy
  --> src/mdtest_snippet.py:31:4
   |
31 | if wut:  # snapshot: redundant-condition
   |    ^^^ Did you mean to call this function?
   |
30 |
   - if wut:  # snapshot: redundant-condition
31 + if wut(...):  # snapshot: redundant-condition
32 |     pass
   |
note: This is a display-only fix and is likely to be incorrect


warning[redundant-condition]: Function `wuttt` is always truthy
  --> src/mdtest_snippet.py:36:8
   |
36 |     if wuttt:  # snapshot: redundant-condition
   |        ^^^^^ Did you mean to `await` and call this function?
   |
35 | async def bar():
   -     if wuttt:  # snapshot: redundant-condition
36 +     if await wuttt(...):  # snapshot: redundant-condition
37 |         pass
   |
note: This is a display-only fix and is likely to be incorrect
```

Synchronous functions returning `Any`, an inferred `Unknown`, or `Never` are not known to return
coroutines. We suggest calling them without adding `await`, even inside an asynchronous function.

```py
def unannotated():
    return False

def dynamic() -> Any:
    return False

def terminate() -> Never:
    raise RuntimeError

async def check_synchronous_functions():
    if unannotated:  # snapshot: redundant-condition
        pass
    if dynamic:  # snapshot: redundant-condition
        pass
    if terminate:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Function `unannotated` is always truthy
  --> src/mdtest_snippet.py:48:8
   |
48 |     if unannotated:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^ Did you mean to call this function?
   |
47 | async def check_synchronous_functions():
   -     if unannotated:  # snapshot: redundant-condition
48 +     if unannotated():  # snapshot: redundant-condition
49 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Function `dynamic` is always truthy
  --> src/mdtest_snippet.py:50:8
   |
50 |     if dynamic:  # snapshot: redundant-condition
   |        ^^^^^^^ Did you mean to call this function?
   |
49 |         pass
   -     if dynamic:  # snapshot: redundant-condition
50 +     if dynamic():  # snapshot: redundant-condition
51 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Function `terminate` is always truthy
  --> src/mdtest_snippet.py:52:8
   |
52 |     if terminate:  # snapshot: redundant-condition
   |        ^^^^^^^^^ Did you mean to call this function?
   |
51 |         pass
   -     if terminate:  # snapshot: redundant-condition
52 +     if terminate():  # snapshot: redundant-condition
53 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

A synchronous function can explicitly return a coroutine. Calling and awaiting that function is a
valid suggestion, while testing a call that never returns has no condition to diagnose.

```py
def make_coroutine() -> CoroutineType[Any, Any, bool]:
    return always_truthy_coro()

async def check_coroutine_factory():
    if make_coroutine:  # snapshot: redundant-condition
        pass

async def check_nonreturning_call():
    if terminate():
        pass
```

```snapshot
warning[redundant-condition]: Function `make_coroutine` is always truthy
  --> src/mdtest_snippet.py:58:8
   |
58 |     if make_coroutine:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^ Did you mean to `await` and call this function?
   |
57 | async def check_coroutine_factory():
   -     if make_coroutine:  # snapshot: redundant-condition
58 +     if await make_coroutine():  # snapshot: redundant-condition
59 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

An awaitable in a synchronous function or a lambda still produces a diagnostic, but suggesting
`await` would create invalid syntax, so we also do not add an autofix here:

```py
def inspect_synchronous_awaitable():
    if coroutine():  # snapshot: redundant-condition
        pass

async def inspect_lambda_awaitable():
    return lambda: True if coroutine() else False  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:65:8
   |
65 |     if coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:69:28
   |
69 |     return lambda: True if coroutine() else False  # snapshot: redundant-condition
   |                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
```

Awaiting an expression is valid within a comprehension in an asynchronous function or within a
generator expression:

```py
async def inspect_comprehension_awaitable():
    return [value for value in range(1) if coroutine()]  # snapshot: redundant-condition

def inspect_generator_awaitable():
    return (value for value in range(1) if coroutine())  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:71:44
   |
71 |     return [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
   |                                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
70 | async def inspect_comprehension_awaitable():
   -     return [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
71 +     return [value for value in range(1) if await coroutine()]  # snapshot: redundant-condition
72 |
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:74:44
   |
74 |     return (value for value in range(1) if coroutine())  # snapshot: redundant-condition
   |                                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
73 | def inspect_generator_awaitable():
   -     return (value for value in range(1) if coroutine())  # snapshot: redundant-condition
74 +     return (value for value in range(1) if await coroutine())  # snapshot: redundant-condition
75 | async def inspect_named_awaitable():
   |
note: This is an unsafe fix and may change runtime behavior
```

Assignment expressions need parentheses so the assignment still happens before awaiting its result:

```py
async def inspect_named_awaitable():
    if value := coroutine():  # snapshot: redundant-condition-strict
        pass
```

```snapshot
error[redundant-condition-strict]: Condition is always truthy
  --> src/mdtest_snippet.py:76:8
   |
76 |     if value := coroutine():  # snapshot: redundant-condition-strict
   |        ^^^^^^^^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
75 | async def inspect_named_awaitable():
   -     if value := coroutine():  # snapshot: redundant-condition-strict
76 +     if await (value := coroutine()):  # snapshot: redundant-condition-strict
77 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

Unary and binary operations need parentheses so the entire original expression is awaited:

```py
class AwaitableOperations:
    async def __neg__(self) -> bool:
        return True

    async def __add__(self, other: object) -> bool:
        return True

async def inspect_awaitable_operations(value: AwaitableOperations):
    if -value:  # snapshot: redundant-condition
        pass

    if value + value:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:86:8
   |
86 |     if -value:  # snapshot: redundant-condition
   |        ^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
85 | async def inspect_awaitable_operations(value: AwaitableOperations):
   -     if -value:  # snapshot: redundant-condition
86 +     if await (-value):  # snapshot: redundant-condition
87 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:89:8
   |
89 |     if value + value:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
88 |
   -     if value + value:  # snapshot: redundant-condition
89 +     if await (value + value):  # snapshot: redundant-condition
90 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

When a conditional expression is tested for truthiness, each awaitable branch receives its own
`await` fix:

```py
async def inspect_conditional_awaitable(flag: bool):
    if (
        coroutine()  # snapshot: redundant-condition
        if flag
        else coroutine()  # snapshot: redundant-condition
    ):
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:93:9
   |
93 |         coroutine()  # snapshot: redundant-condition
   |         ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
92 |     if (
   -         coroutine()  # snapshot: redundant-condition
93 +         await coroutine()  # snapshot: redundant-condition
94 |         if flag
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:95:14
   |
95 |         else coroutine()  # snapshot: redundant-condition
   |              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
94 |         if flag
   -         else coroutine()  # snapshot: redundant-condition
95 +         else await coroutine()  # snapshot: redundant-condition
96 |     ):
   |
note: This is an unsafe fix and may change runtime behavior
```

An expression that has already been awaited needs parentheses before adding another `await`:

```py
async def nested_coroutine() -> CoroutineType[Any, Any, bool]:
    return coroutine()

async def inspect_nested_awaitable():
    if await nested_coroutine():  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:102:8
    |
102 |     if await nested_coroutine():  # snapshot: redundant-condition
    |        ^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
    |
101 | async def inspect_nested_awaitable():
    -     if await nested_coroutine():  # snapshot: redundant-condition
102 +     if await (await nested_coroutine()):  # snapshot: redundant-condition
103 |         pass
    |
note: This is an unsafe fix and may change runtime behavior
```

Annotations, type aliases, type-parameter bounds, and generic class bases cannot contain `await`,
even when they appear inside an asynchronous function. Their diagnostics therefore have no autofix:

```py
from typing import Annotated

class Base: ...

async def inspect_restricted_awaitable_contexts():
    type Alias = Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition

    class Generic[T: Annotated[int, 1 if coroutine() else 0]]:  # snapshot: redundant-condition
        pass

    def generic[T: Annotated[int, 1 if coroutine() else 0]]():  # snapshot: redundant-condition
        pass

    type GenericAlias[T: Annotated[int, 1 if coroutine() else 0]] = list[T]  # snapshot: redundant-condition

    class GenericBase[T](Base if coroutine() else Base):  # snapshot: redundant-condition
        pass

    def nested(value: Annotated[int, 1 if coroutine() else 0]):  # snapshot: redundant-condition
        pass

    def returned() -> Annotated[int, 1 if coroutine() else 0]:  # snapshot: redundant-condition
        return 1

    variable: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition

    list_comprehension: Annotated[int, [value for value in range(1) if coroutine()]]  # snapshot: redundant-condition
    set_comprehension: Annotated[int, {value for value in range(1) if coroutine()}]  # snapshot: redundant-condition
    dict_comprehension: Annotated[int, {value: value for value in range(1) if coroutine()}]  # snapshot: redundant-condition

    def nested_comprehension(
        value: Annotated[int, [item for item in range(1) if coroutine()]],  # snapshot: redundant-condition
    ):
        pass

    def returned_comprehension() -> Annotated[
        int, [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
    ]:
        return 1

class AnnotatedHolder:
    async def inspect(self):
        self.value: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:109:38
    |
109 |     type Alias = Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
    |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:111:42
    |
111 |     class Generic[T: Annotated[int, 1 if coroutine() else 0]]:  # snapshot: redundant-condition
    |                                          ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:114:40
    |
114 |     def generic[T: Annotated[int, 1 if coroutine() else 0]]():  # snapshot: redundant-condition
    |                                        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:117:46
    |
117 |     type GenericAlias[T: Annotated[int, 1 if coroutine() else 0]] = list[T]  # snapshot: redundant-condition
    |                                              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:119:34
    |
119 |     class GenericBase[T](Base if coroutine() else Base):  # snapshot: redundant-condition
    |                                  ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:122:43
    |
122 |     def nested(value: Annotated[int, 1 if coroutine() else 0]):  # snapshot: redundant-condition
    |                                           ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:125:43
    |
125 |     def returned() -> Annotated[int, 1 if coroutine() else 0]:  # snapshot: redundant-condition
    |                                           ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:128:35
    |
128 |     variable: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
    |                                   ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:130:72
    |
130 |     list_comprehension: Annotated[int, [value for value in range(1) if coroutine()]]  # snapshot: redundant-condition
    |                                                                        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:131:71
    |
131 |     set_comprehension: Annotated[int, {value for value in range(1) if coroutine()}]  # snapshot: redundant-condition
    |                                                                       ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:132:79
    |
132 |     dict_comprehension: Annotated[int, {value: value for value in range(1) if coroutine()}]  # snapshot: redundant-condition
    |                                                                               ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:135:61
    |
135 |         value: Annotated[int, [item for item in range(1) if coroutine()]],  # snapshot: redundant-condition
    |                                                             ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:140:46
    |
140 |         int, [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
    |                                              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:146:41
    |
146 |         self.value: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
    |                                         ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
```

A generator expression introduces a scope where `await` is valid even when the generator appears
inside an annotation. This also permits awaiting inside a comprehension nested in that generator:

```py
async def inspect_generator_annotations():
    direct: Annotated[int, (value for value in range(1) if coroutine())]  # snapshot: redundant-condition
    nested: Annotated[int, ([value for value in range(1) if coroutine()] for _ in range(1))]  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:148:60
    |
148 |     direct: Annotated[int, (value for value in range(1) if coroutine())]  # snapshot: redundant-condition
    |                                                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
147 | async def inspect_generator_annotations():
    -     direct: Annotated[int, (value for value in range(1) if coroutine())]  # snapshot: redundant-condition
148 +     direct: Annotated[int, (value for value in range(1) if await coroutine())]  # snapshot: redundant-condition
149 |     nested: Annotated[int, ([value for value in range(1) if coroutine()] for _ in range(1))]  # snapshot: redundant-condition
    |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:149:61
    |
149 |     nested: Annotated[int, ([value for value in range(1) if coroutine()] for _ in range(1))]  # snapshot: redundant-condition
    |                                                             ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
148 |     direct: Annotated[int, (value for value in range(1) if coroutine())]  # snapshot: redundant-condition
    -     nested: Annotated[int, ([value for value in range(1) if coroutine()] for _ in range(1))]  # snapshot: redundant-condition
149 +     nested: Annotated[int, ([value for value in range(1) if await coroutine()] for _ in range(1))]  # snapshot: redundant-condition
150 | async def inspect_allowed_definition_awaitables():
    |
note: This is an unsafe fix and may change runtime behavior
```

Non-generic class bases and function parameter defaults can contain `await` when they are evaluated
in an asynchronous function, even if the function being defined has type parameters:

```py
async def inspect_allowed_definition_awaitables():
    class NongenericBase(Base if coroutine() else Base):  # snapshot: redundant-condition
        pass

    def generic_default[T](value: int = 1 if coroutine() else 0):  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:151:34
    |
151 |     class NongenericBase(Base if coroutine() else Base):  # snapshot: redundant-condition
    |                                  ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
150 | async def inspect_allowed_definition_awaitables():
    -     class NongenericBase(Base if coroutine() else Base):  # snapshot: redundant-condition
151 +     class NongenericBase(Base if await coroutine() else Base):  # snapshot: redundant-condition
152 |         pass
    |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:154:46
    |
154 |     def generic_default[T](value: int = 1 if coroutine() else 0):  # snapshot: redundant-condition
    |                                              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
153 |
    -     def generic_default[T](value: int = 1 if coroutine() else 0):  # snapshot: redundant-condition
154 +     def generic_default[T](value: int = 1 if await coroutine() else 0):  # snapshot: redundant-condition
155 |         pass
    |
note: This is an unsafe fix and may change runtime behavior
```

Type expressions used as runtime values and the values of annotated assignments are ordinary Python
expressions, so they can contain `await` inside an asynchronous function:

```py
async def inspect_runtime_type_expressions():
    alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
    value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:157:38
    |
157 |     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
    |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
156 | async def inspect_runtime_type_expressions():
    -     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
157 +     alias = list[Annotated[int, 1 if await coroutine() else 0]]  # snapshot: redundant-condition
158 |     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
    |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:158:23
    |
158 |     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
    |                       ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
157 |     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
    -     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
158 +     value: int = 1 if await coroutine() else 0  # snapshot: redundant-condition
159 | async def inspect_compound_awaitable(flag: bool):
    |
note: This is an unsafe fix and may change runtime behavior
```

An awaitable in the final operand of a compound condition still receives an autofix when the
condition as a whole has ambiguous truthiness:

```py
async def inspect_compound_awaitable(flag: bool):
    if flag and coroutine():  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:160:17
    |
160 |     if flag and coroutine():  # snapshot: redundant-condition
    |                 ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
159 | async def inspect_compound_awaitable(flag: bool):
    -     if flag and coroutine():  # snapshot: redundant-condition
160 +     if flag and await coroutine():  # snapshot: redundant-condition
161 |         pass
    |
note: This is an unsafe fix and may change runtime behavior
```

Python modules do not allow top-level `await`, so awaitable conditions at module scope have no
autofix:

```py
if coroutine():  # snapshot: redundant-condition
    pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:162:4
    |
162 | if coroutine():  # snapshot: redundant-condition
    |    ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
```

## `await` fixes in nested comprehensions before Python 3.11

Before Python 3.11, an asynchronous comprehension cannot implicitly make its containing
comprehension or generator expression asynchronous. Adding `await` in these nested conditions would
therefore produce invalid syntax, so their diagnostics have no autofix.

```toml
[environment]
python-version = "3.10"
python-platform = "linux"

[rules]
redundant-condition-strict = "error"
```

```py
async def predicate() -> bool:
    return False

def nested_in_generators():
    lists = ([item for item in [1] if predicate()] for _ in [1])  # snapshot: redundant-condition
    sets = ({item for item in [1] if predicate()} for _ in [1])  # snapshot: redundant-condition
    dicts = ({item: item for item in [1] if predicate()} for _ in [1])  # snapshot: redundant-condition

async def nested_in_list():
    return [[item for item in [1] if predicate()] for _ in [1]]  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
 --> src/mdtest_snippet.py:5:39
  |
5 |     lists = ([item for item in [1] if predicate()] for _ in [1])  # snapshot: redundant-condition
  |                                       ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`


warning[redundant-condition]: Condition is always truthy
 --> src/mdtest_snippet.py:6:38
  |
6 |     sets = ({item for item in [1] if predicate()} for _ in [1])  # snapshot: redundant-condition
  |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`


warning[redundant-condition]: Condition is always truthy
 --> src/mdtest_snippet.py:7:45
  |
7 |     dicts = ({item: item for item in [1] if predicate()} for _ in [1])  # snapshot: redundant-condition
  |                                             ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:10:38
   |
10 |     return [[item for item in [1] if predicate()] for _ in [1]]  # snapshot: redundant-condition
   |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
```

A containing generator that already uses `await` is asynchronous, so awaiting a nested condition is
valid even on Python 3.10. A condition directly inside a generator also remains eligible.

```py
def already_async_generator():
    return ([item for item in [1] if predicate()] for _ in [1] if await predicate())  # snapshot: redundant-condition

def direct_generator():
    return (item for item in [1] if predicate())  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:12:38
   |
12 |     return ([item for item in [1] if predicate()] for _ in [1] if await predicate())  # snapshot: redundant-condition
   |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
11 | def already_async_generator():
   -     return ([item for item in [1] if predicate()] for _ in [1] if await predicate())  # snapshot: redundant-condition
12 +     return ([item for item in [1] if await predicate()] for _ in [1] if await predicate())  # snapshot: redundant-condition
13 |
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:15:37
   |
15 |     return (item for item in [1] if predicate())  # snapshot: redundant-condition
   |                                     ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
14 | def direct_generator():
   -     return (item for item in [1] if predicate())  # snapshot: redundant-condition
15 +     return (item for item in [1] if await predicate())  # snapshot: redundant-condition
   |
note: This is an unsafe fix and may change runtime behavior
```

## `await` fixes in nested comprehensions on Python 3.11

Python 3.11 allows a nested asynchronous comprehension to make its enclosing comprehension or
generator expression asynchronous. Both conditions below can therefore receive an `await` fix.

```toml
[environment]
python-version = "3.11"
python-platform = "linux"

[rules]
redundant-condition-strict = "error"
```

```py
async def predicate() -> bool:
    return False

def nested_in_generator():
    return ([item for item in [1] if predicate()] for _ in [1])  # snapshot: redundant-condition

async def nested_in_list():
    return [[item for item in [1] if predicate()] for _ in [1]]  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
 --> src/mdtest_snippet.py:5:38
  |
5 |     return ([item for item in [1] if predicate()] for _ in [1])  # snapshot: redundant-condition
  |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
  |
4 | def nested_in_generator():
  -     return ([item for item in [1] if predicate()] for _ in [1])  # snapshot: redundant-condition
5 +     return ([item for item in [1] if await predicate()] for _ in [1])  # snapshot: redundant-condition
6 |
  |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
 --> src/mdtest_snippet.py:8:38
  |
8 |     return [[item for item in [1] if predicate()] for _ in [1]]  # snapshot: redundant-condition
  |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
  |
7 | async def nested_in_list():
  -     return [[item for item in [1] if predicate()] for _ in [1]]  # snapshot: redundant-condition
8 +     return [[item for item in [1] if await predicate()] for _ in [1]]  # snapshot: redundant-condition
  |
note: This is an unsafe fix and may change runtime behavior
```

## Notebook cells

Notebook cells do allow top-level `await`, so the same condition receives an autofix there:

```ipynb
{
  "cells": [
    {
      "cell_type": "code",
      "execution_count": null,
      "metadata": {},
      "outputs": [],
      "source": [
        "async def coroutine() -> bool:\n",
        "    return False\n",
        "\n",
        "if coroutine():  # snapshot: redundant-condition\n",
        "    pass\n"
      ]
    }
  ],
  "metadata": {},
  "nbformat": 4,
  "nbformat_minor": 4
}
```

```snapshot
warning[redundant-condition]: Condition is always truthy
 --> src/mdtest_snippet.ipynb:cell 1:4:4
  |
4 | if coroutine():  # snapshot: redundant-condition
  |    ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
 ::: cell 1
  |
3 |
  - if coroutine():  # snapshot: redundant-condition
4 + if await coroutine():  # snapshot: redundant-condition
5 |     pass
  |
note: This is an unsafe fix and may change runtime behavior
```

## Strict version

Our stricter `redundant-condition-strict` rule extends this logic to boolean and integer tests:

```py
from typing import Literal

def f(x: Literal[1, 2]):
    if x > 5:  # error: [redundant-condition-strict]
        pass

    if x:  # snapshot: redundant-condition-strict
        pass

def g(flag: bool, some_bytes: bytes):
    if flag:
        pass
    elif some_bytes[0] == b"\x1e":  # snapshot: redundant-condition-strict
        pass
```

```snapshot
error[redundant-condition-strict]: Condition is always truthy
 --> src/mdtest_snippet.py:7:8
  |
7 |     if x:  # snapshot: redundant-condition-strict
  |        ^ Inferred type is `Literal[1, 2]`


error[redundant-condition-strict]: Condition is always false
  --> src/mdtest_snippet.py:13:10
   |
13 |     elif some_bytes[0] == b"/x1e":  # snapshot: redundant-condition-strict
   |          -------------^^^^-------
   |          |                |
   |          |                Has type `Literal[b"/x1e"]`
   |          Has type `int`
```

We offer bespoke diagnostics for common mistakes such as accidentally comparing a string with a
bytestring:

```py
def falsy(flag: bool):
    if flag:
        pass
    elif "foo" == b"foo":  # snapshot: redundant-condition-strict
        pass
```

```snapshot
error[redundant-condition-strict]: Condition is always false
  --> src/mdtest_snippet.py:18:10
   |
18 |     elif "foo" == b"foo":  # snapshot: redundant-condition-strict
   |          -----^^^^------
   |          |        |
   |          |        Instance of `bytes`
   |          Instance of `str`
```

Or comparing a number with a string:

```py
x = 1
if x == "1":  # snapshot: redundant-condition-strict
    pass
```

```snapshot
error[redundant-condition-strict]: Condition is always false
  --> src/mdtest_snippet.py:21:4
   |
21 | if x == "1":  # snapshot: redundant-condition-strict
   |    -^^^^---
   |    |    |
   |    |    Instance of `str`
   |    Instance of `int`
```

Or testing the length of a tuple that always has a fixed length:

```py
def test(x: tuple[int]):  # the user probably meant to use `tuple[int, ...]` here
    # error: [redundant-condition-strict] "`x` always has length 1"
    if len(x) == 1:
        pass

    if len(x) == 2:  # snapshot: redundant-condition-strict
        pass
```

```snapshot
error[redundant-condition-strict]: `x` always has length 1
  --> src/mdtest_snippet.py:28:8
   |
23 | def test(x: tuple[int]):  # the user probably meant to use `tuple[int, ...]` here
   |             ----------
   |             |
   |             Inferred as a 1-element tuple due to this annotation
   |             Did you mean `tuple[int, ...]`?
24 |     # error: [redundant-condition-strict] "`x` always has length 1"
25 |     if len(x) == 1:
26 |         pass
27 |
28 |     if len(x) == 2:  # snapshot: redundant-condition-strict
   |        ^^^^-^^^^^^
   |            |
   |            Has type `tuple[int]`
```

We avoid annotating the inferred types of comparison conditions for very obvious AST literals such
as the `None` keyword or number-literal expressions, including signed numbers:

```py
def f(x: None):
    if x is None:  # snapshot: redundant-condition-strict
        pass

    if x == 3:  # snapshot: redundant-condition-strict
        pass

    if x == -3:  # snapshot: redundant-condition-strict
        pass

    if x == +3:  # snapshot: redundant-condition-strict
        pass
```

```snapshot
error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:31:8
   |
31 |     if x is None:  # snapshot: redundant-condition-strict
   |        -^^^^^^^^
   |        |
   |        Has type `None`


error[redundant-condition-strict]: Condition is always false
  --> src/mdtest_snippet.py:34:8
   |
34 |     if x == 3:  # snapshot: redundant-condition-strict
   |        -^^^^^
   |        |
   |        Has type `None`


error[redundant-condition-strict]: Condition is always false
  --> src/mdtest_snippet.py:37:8
   |
37 |     if x == -3:  # snapshot: redundant-condition-strict
   |        -^^^^^^
   |        |
   |        Has type `None`


error[redundant-condition-strict]: Condition is always false
  --> src/mdtest_snippet.py:40:8
   |
40 |     if x == +3:  # snapshot: redundant-condition-strict
   |        -^^^^^^
   |        |
   |        Has type `None`
```

`redundant-condition-strict` is also emitted on negated conditions where the negated condition is
inferred as an instance of `bool`:

```py
def negated_conditions():
    if not 1 > 2:  # error: [redundant-condition-strict] "Condition `not 1 > 2` is always true"
        pass

    if not 1 < 2:  # error: [redundant-condition-strict] "Condition `not 1 < 2` is always false"
        pass

    if not 0 == 1:  # error: [redundant-condition-strict] "Condition `not 0 == 1` is always true"
        pass

    if not 1 == 1:  # error: [redundant-condition-strict] "Condition `not 1 == 1` is always false"
        pass

    if not not 1 == 1:  # error: [redundant-condition-strict] "Condition `not not 1 == 1` is always true"
        pass

def negated_conditional_contexts(flag: bool):
    if flag:
        pass
    elif not 1 == 0:  # error: [redundant-condition-strict] "Condition `not 1 == 0` is always true"
        pass

    while not 1 == 0:  # error: [redundant-condition-strict] "Condition `not 1 == 0` is always true"
        break
```

Outside a statement condition, a `not` expression still tests its operand's truthiness. The strict
rule reports redundant boolean and integer operands in assignments and return expressions:

```py
def negated_boolean_assignment(value: str):
    result = not isinstance(value, str)  # error: [redundant-condition-strict] "Condition `isinstance(value, str)` is always true"

def negated_integer_return(value: Literal[1, 2]) -> bool:
    return not value  # error: [redundant-condition-strict] "Object of type `Literal[1, 2]` is always truthy"
```

When the strict rule is needed because of a test's type or short-circuit behavior, we report the
complete compound condition instead of its operands. Only a single diagnostic is emitted on each of
these:

```py
def compound_truthy(x: str):
    if isinstance(x, str) and isinstance(x, str):  # error: [redundant-condition-strict]
        pass

    while isinstance(x, str) and isinstance(x, str):  # error: [redundant-condition-strict]
        break

    match x:
        case str() if isinstance(x, str) and isinstance(x, str):  # error: [redundant-condition-strict]
            pass
```

The suppression currently hides redundant operands when the whole condition has ambiguous
truthiness. This is a current limitation of the rule that we would ideally fix at some point in the
future:

```py
def check(value: int, enabled: bool):
    # TODO: Ideally, flag `value is not None`. This is deferred for now because the strict
    # rule suppresses compound-condition operands to avoid duplicate diagnostics. Avoiding
    # false positives is more important than avoiding false negatives.
    if enabled and value is not None:
        print(value)
```

## Compound conditions with mixed value types

Reporting a subexpression under `redundant-condition` takes precedence over reporting the complete
condition under `redundant-condition-strict`. Negating the condition does not add a second
diagnostic for the same subexpression.

```py
def func(): ...
def mixed_operands(value: object):
    if func and False:  # error: [redundant-condition] "Function `func` is always truthy"
        pass
    if not (value or func):  # error: [redundant-condition] "Function `func` is always truthy"
        pass
```

When neither operand is reported, the strict rule can report a fixed outcome established by
short-circuit evaluation, even if the expression's value type has ambiguous truthiness.

```py
def short_circuit(value: object):
    if value and False:  # error: [redundant-condition-strict] "Condition `value and False` is always false"
        pass
```

## Boolean tests inside value expressions

A call's arguments compute values, but can contain their own boolean tests. Those tests are checked
even when the call itself has ambiguous truthiness.

```py
def func(): ...
def accepts(value: object) -> bool:
    return bool(value)

def nested_tests():
    if accepts(not func):  # error: [redundant-condition]
        pass
```

`lambda` bodies and comprehension filters have their own scopes. `lambda` defaults and a
comprehension's first iterable are evaluated in the enclosing scope. Each nested boolean test is
reported once in either case.

```py
def nested_scopes():
    if accepts(lambda: not func):  # error: [redundant-condition]
        pass
    if accepts(lambda value=not func: value):  # error: [redundant-condition]
        pass
    if accepts([item for item in (not func,)]):  # error: [redundant-condition]
        pass
    if accepts([item for item in range(2) if not func]):  # error: [redundant-condition]
        pass
```

Compound conditions in conditional expressions and comprehension filters also report the complete
condition once, rather than both the condition and its negated operand.

```py
def compound_expression_tests():
    selected = 1 if not not (1 == 1) else 0  # error: [redundant-condition-strict] "Condition `not not (1 == 1)` is always true"
    filtered = [
        item
        for item in range(2)
        # error: [redundant-condition-strict] "Condition `not not (1 == 1)` is always true"
        if not not (1 == 1)
    ]
```

The selected values can also contain independent boolean tests, on either side of the condition in
source order.

```py
def selected_values(flag: bool):
    # snapshot: redundant-condition
    # snapshot: redundant-condition
    selected = not func if flag else not func
```

```snapshot
warning[redundant-condition]: Function `func` is always truthy
  --> src/mdtest_snippet.py:28:20
   |
28 |     selected = not func if flag else not func
   |                    ^^^^ Did you mean to call this function?
   |
27 |     # snapshot: redundant-condition
   -     selected = not func if flag else not func
28 +     selected = not func() if flag else not func
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Function `func` is always truthy
  --> src/mdtest_snippet.py:28:42
   |
28 |     selected = not func if flag else not func
   |                                          ^^^^ Did you mean to call this function?
   |
27 |     # snapshot: redundant-condition
   -     selected = not func if flag else not func
28 +     selected = not func if flag else not func()
   |
note: This is an unsafe fix and may change runtime behavior
```

## Multiline conditions in concise diagnostics

Concise diagnostics usually quote source code in their diagnostics:

```py
if 1 + 1 == 2:  # error: [redundant-condition-strict] "Condition `1 + 1 == 2` is always true"
    pass
```

But the source code is omitted if the full condition is split over multiple lines:

```py
def multiline_conditions(value: int):
    if (
        value is not None  # error: [redundant-condition-strict] "Condition is always true"
        # Both operands are always true.
        and value is not None
    ):
        pass

    # fmt: off
    if (value  # error: [redundant-condition-strict] "Condition is always false"
        is
        None):
        pass
    # fmt: on
```

## Adding an exhaustiveness check after a redundant final `elif`

```toml
[environment]
python-version = "3.11"

[rules]
redundant-condition-strict = "error"
```

When a final `elif` condition is always true, an `else` branch calling `assert_never` makes the
exhaustiveness check explicit. The argument is a variable that narrows to `Never` when the condition
is false. The original condition and body are preserved:

```py
def exhaustive(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
        print(value + 1)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:4:10
  |
4 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
   |
1  + from typing import assert_never
2  | def exhaustive(value: str | int):
--------------------------------------------------------------------------------
7  |         print(value + 1)
8  +     else:
9  +         assert_never(value)
10 | # fmt: off
   |
note: This is an unsafe fix and may change runtime behavior
```

The assertion uses the existing indentation of the branch body, including unconventional
indentation:

```py
# fmt: off
def unconventional_indentation(value: str | int):
  if isinstance(value, str):
    print(value)
  elif isinstance(value, int):  # snapshot: redundant-condition-strict
    print(value)
# fmt: on
```

```snapshot
error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:11:8
   |
11 |   elif isinstance(value, int):  # snapshot: redundant-condition-strict
   |        ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
   |
1  + from typing import assert_never
2  | def exhaustive(value: str | int):
--------------------------------------------------------------------------------
13 |     print(value)
14 +   else:
15 +     assert_never(value)
16 | # fmt: on
   |
note: This is an unsafe fix and may change runtime behavior
```

Comments inside a parenthesized condition, after the branch header, and in its body are all
preserved. Trailing body comments remain before the new `else`:

```py
def commented_condition(value: str | int):
    if isinstance(value, str):
        print(value)
    elif (
        # Explain the defensive runtime check.
        isinstance(value, int)  # snapshot: redundant-condition-strict
    ):  # Preserve this header comment.
        # Preserve this body comment.
        print(value)
        # Preserve this trailing body comment.
```

```snapshot
error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:19:9
   |
19 |         isinstance(value, int)  # snapshot: redundant-condition-strict
   |         ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
   |
1  + from typing import assert_never
2  | def exhaustive(value: str | int):
--------------------------------------------------------------------------------
24 |         # Preserve this trailing body comment.
25 +     else:
26 +         assert_never(value)
27 | def assignment_expression(value: str | int):
   |
note: This is an unsafe fix and may change runtime behavior
```

An unparenthesized assignment expression is valid in an `elif` condition but must be parenthesized
when moved into an assertion:

```py
def assignment_expression(value: str | int):
    if isinstance(value, str):
        print(value)
    elif matched := isinstance(value, int):  # snapshot: redundant-condition-strict
        print(matched)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:27:10
   |
27 |     elif matched := isinstance(value, int):  # snapshot: redundant-condition-strict
   |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`
   |
26 |         print(value)
   -     elif matched := isinstance(value, int):  # snapshot: redundant-condition-strict
27 +     else:  # snapshot: redundant-condition-strict
28 +         assert (matched := isinstance(value, int))
29 |         print(matched)
   |
note: This is an unsafe fix and may change runtime behavior
```

If the branch body begins on the same line as its header, the new `else` still goes on a separate
line. Its body uses the file's indentation style:

```py
# fmt: off
def inline_branch(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int): print(value)  # snapshot: redundant-condition-strict
# fmt: on
```

```snapshot
error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:33:10
   |
33 |     elif isinstance(value, int): print(value)  # snapshot: redundant-condition-strict
   |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
   |
1  + from typing import assert_never
2  | def exhaustive(value: str | int):
--------------------------------------------------------------------------------
34 |     elif isinstance(value, int): print(value)  # snapshot: redundant-condition-strict
35 +     else:
36 +         assert_never(value)
37 | # fmt: on
   |
note: This is an unsafe fix and may change runtime behavior
```

A multiline header can also have a body on the same line as its closing colon:

```py
# fmt: off
def multiline_inline_branch(value: str | int):
    if isinstance(value, str):
        print(value)
    elif (
        isinstance(value, int)  # snapshot: redundant-condition-strict
    ): print(value)
# fmt: on
```

```snapshot
error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:40:9
   |
40 |         isinstance(value, int)  # snapshot: redundant-condition-strict
   |         ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
   |
1  + from typing import assert_never
2  | def exhaustive(value: str | int):
--------------------------------------------------------------------------------
42 |     ): print(value)
43 +     else:
44 +         assert_never(value)
45 | # fmt: on
   |
note: This is an unsafe fix and may change runtime behavior
```

Parser recovery can produce an `elif` branch with no statements. The redundant condition is still
reported, but no autofix is offered for the incomplete branch:

```py
def empty_branch(value: str | int):
    if isinstance(value, str):
        print(value)
    # error: [invalid-syntax] "Expected an indented block after `elif` clause"
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
```

```snapshot
error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:47:10
   |
47 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
   |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`
```

A boolean `elif` following a non-boolean condition still receives the assertion fix. The type of the
first condition does not determine how later conditions are diagnosed.

```py
def non_boolean_first_condition(items: list[int], value: int):
    if items:
        print(items)
    elif value is not None:  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:51:10
   |
51 |     elif value is not None:  # snapshot: redundant-condition-strict
   |          -----^^^^^^^^^^^^
   |          |
   |          Has type `int`
help: Add an `else` branch that calls `assert_never`
   |
1  + from typing import assert_never
2  | def exhaustive(value: str | int):
--------------------------------------------------------------------------------
52 |     elif value is not None:  # snapshot: redundant-condition-strict
   -         print(value)
53 +         print(value)
54 +     else:
55 +         assert_never(value)
   |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks for comparisons

Equality comparisons can narrow a literal union to `Never`. The new assertion reuses the tested
variable rather than evaluating the comparison again:

```py
from typing import Literal

def exhaustive(value: Literal["a", "b"]):
    if value == "a":
        print(value)
    elif "b" == value:  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:6:10
  |
6 |     elif "b" == value:  # snapshot: redundant-condition-strict
  |          ---^^^^-----
  |          |      |
  |          |      Has type `Literal["b"]`
  |          Has type `Literal["b"]`
help: Add an `else` branch that calls `assert_never`
  |
  - from typing import Literal
1 + from typing import Literal, assert_never
2 |
--------------------------------------------------------------------------------
6 |     elif "b" == value:  # snapshot: redundant-condition-strict
  -         print(value)
7 +         print(value)
8 +     else:
9 +         assert_never(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks with imported aliases

An existing runtime import of `assert_never` can be reused, including an alias:

```py
from typing import assert_never as unreachable

def exhaustive(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:6:10
  |
6 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
  |
6 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  -         print(value)
7 +         print(value)
8 +     else:
9 +         unreachable(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks with qualified imports

A qualified module import can also be reused. The assertion goes after the entire branch body,
including nested statements:

```py
import typing as t

def exhaustive(value: str | int, flag: bool):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        if flag:
            print(value)
        # This comment belongs to the `elif` body.
    print("done")
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:6:10
  |
6 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
   |
9  |         # This comment belongs to the `elif` body.
10 +     else:
11 +         t.assert_never(value)
12 |     print("done")
   |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks with a shadowed function name

When `assert_never` is already bound, a qualified import avoids that binding:

```py
def exhaustive(value: str | int, assert_never: int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value, assert_never)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:4:10
  |
4 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
  |
1 + import typing
2 | def exhaustive(value: str | int, assert_never: int):
3 |     if isinstance(value, str):
4 |         print(value)
5 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  -         print(value, assert_never)
6 +         print(value, assert_never)
7 +     else:
8 +         typing.assert_never(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks with shadowed imports

When both the function and module names are shadowed, the fix retains the original defensive
assertion. An import elsewhere in the module does not make a shadowed alias usable:

```py
import typing as t
from typing import assert_never

def exhaustive(value: str | int, t: int, typing: int, assert_never: int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:7:10
  |
7 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`
  |
6 |         print(value)
  -     elif isinstance(value, int):  # snapshot: redundant-condition-strict
7 +     else:  # snapshot: redundant-condition-strict
8 +         assert isinstance(value, int)
9 |         print(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks with a deleted import

An imported alias that has been deleted cannot be reused. A new qualified import provides a runtime
binding for the assertion:

```py
from typing import assert_never as unreachable

del unreachable

def exhaustive(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:8:10
  |
8 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
   |
1  + import typing
2  | from typing import assert_never as unreachable
--------------------------------------------------------------------------------
9  |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
   -         print(value)
10 +         print(value)
11 +     else:
12 +         typing.assert_never(value)
   |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks without a reusable variable

Calling a function again could change its result or have side effects, so the fix retains the
original condition as an assertion when the tested value is not a plain variable:

```py
def get_value() -> int:
    return 1

def exhaustive(flag: bool):
    if flag:
        print("flag")
    elif isinstance(get_value(), int):  # snapshot: redundant-condition-strict
        print("integer")
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:7:10
  |
7 |     elif isinstance(get_value(), int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`
  |
6 |         print("flag")
  -     elif isinstance(get_value(), int):  # snapshot: redundant-condition-strict
7 +     else:  # snapshot: redundant-condition-strict
8 +         assert isinstance(get_value(), int)
9 |         print("integer")
  |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks on Python 3.10 without dependency metadata

Python 3.10 does not provide `typing.assert_never`. Bundled stubs for `typing_extensions` do not
establish that the runtime package is available, so the fix retains the original assertion:

```toml
[environment]
python-version = "3.10"

[rules]
redundant-condition-strict = "error"
```

```py
def exhaustive(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/mdtest_snippet.py:4:10
  |
4 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`
  |
3 |         print(value)
  -     elif isinstance(value, int):  # snapshot: redundant-condition-strict
4 +     else:  # snapshot: redundant-condition-strict
5 +         assert isinstance(value, int)
6 |         print(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks with a direct backport dependency

On older Python versions, `assert_never` can be imported from `typing_extensions` when it is a
direct dependency and the installed runtime module exports the function. Bundled stubs alone do not
establish runtime availability:

```toml
[environment]
python-version = "3.10"
python = "/.venv"

[rules]
redundant-condition-strict = "error"

[dependency-metadata]
projects = [{ path = "/src", dependencies = ["extensions"] }]

[dependency-metadata.distributions]
extensions = { name = "typing-extensions" }

[dependency-metadata.module-owners]
typing_extensions = ["extensions"]
```

### Available runtime function

This installed backport provides `assert_never`, so the fix can import it:

`/.venv/<path-to-site-packages>/typing_extensions.py`:

```py
def assert_never(value):
    raise AssertionError(value)
```

`main.py`:

```py
def exhaustive(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/main.py:4:10
  |
4 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Add an `else` branch that calls `assert_never`
  |
1 + from typing_extensions import assert_never
2 | def exhaustive(value: str | int):
3 |     if isinstance(value, str):
4 |         print(value)
5 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  -         print(value)
6 +         print(value)
7 +     else:
8 +         assert_never(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

### Older runtime module

An installed backport without `assert_never` cannot provide the new assertion. The fix retains the
original condition even though the bundled stub includes the function:

`/.venv/<path-to-site-packages>/typing_extensions.py`:

```py
```

`main.py`:

```py
def exhaustive(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/main.py:4:10
  |
4 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`
  |
3 |         print(value)
  -     elif isinstance(value, int):  # snapshot: redundant-condition-strict
4 +     else:  # snapshot: redundant-condition-strict
5 +         assert isinstance(value, int)
6 |         print(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

### Missing runtime module

A dependency declaration does not establish that the backport is installed. If only ty's bundled
stub is available, the fix retains the original condition:

`/.venv/<path-to-site-packages>/unrelated.py`:

```py
```

`main.py`:

```py
def exhaustive(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/main.py:4:10
  |
4 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`
  |
3 |         print(value)
  -     elif isinstance(value, int):  # snapshot: redundant-condition-strict
4 +     else:  # snapshot: redundant-condition-strict
5 +         assert isinstance(value, int)
6 |         print(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

## Exhaustiveness checks with an indirect backport dependency

An installed but undeclared backport does not justify adding a runtime import. The containing
workspace's declaration does not make it a direct dependency of a nested project:

```toml
[environment]
python-version = "3.10"
python = "/.venv"

[rules]
redundant-condition-strict = "error"

[dependency-metadata]
projects = [
    { path = "/src", dependencies = ["extensions"] },
    { path = "/src/member", dependencies = [] },
]

[dependency-metadata.distributions]
extensions = { name = "typing-extensions" }

[dependency-metadata.module-owners]
typing_extensions = ["extensions"]
```

`/.venv/<path-to-site-packages>/typing_extensions.py`:

```py
def assert_never(value):
    raise AssertionError(value)
```

`member/main.py`:

```py
def exhaustive(value: str | int):
    if isinstance(value, str):
        print(value)
    elif isinstance(value, int):  # snapshot: redundant-condition-strict
        print(value)
```

```snapshot
error[redundant-condition-strict]: Condition is always true
 --> src/member/main.py:4:10
  |
4 |     elif isinstance(value, int):  # snapshot: redundant-condition-strict
  |          ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`
  |
3 |         print(value)
  -     elif isinstance(value, int):  # snapshot: redundant-condition-strict
4 +     else:  # snapshot: redundant-condition-strict
5 +         assert isinstance(value, int)
6 |         print(value)
  |
note: This is an unsafe fix and may change runtime behavior
```

## `if` and `while` conditions that use AST literal bools or ints

We maintain a special case for `while` loops, since `while True:` and `while 1:` are common idioms
used to create infinite loops in Python code. Complaining that the conditions `True` and `1` are
"always truthy" in these contexts would obviously be absurd.

```py
def _():
    while True:  # no error
        pass

def _():
    while 1:
        pass  # no error
```

Similarly, some projects use literal `if False:` or `if 0:` in their source code, to mark a region
that is intentionally unreachable, but which could be enabled for debugging purposes. If we see an
*AST literal* used as a condition, rather than a place that is inferred as having a literal *type*,
we suppress the diagnostic: it is assumed that this region is deliberately unreachable.

```py
if False:  # no diagnostic
    pass

if 0:  # no diagnostic
    pass
```

For consistency, we do the same for `if True:`, `if 1:`, `if 2:`, etc.:

```py
if 1:  # no diagnostic
    pass

if True:  # no diagnostic
    pass

if 2:  # no diagnostic
    pass
```

`assert None` also comes up unexpectedly often in certain ecosystem projects to assert an
unreachable region, so we special-case a literal `None` too:

```py
assert None  # no diagnostic
```

## Defensive assertions

The rules are only applied to tests in `assert` statements (and any subexpressions within those
tests) if the inferred type of the `assert` test is not inferred as being a subtype of `bool` or
`int`. This is to prevent false positives on defensive assertions such as the following, which are
common in well written Python code:

```py
def f(x: str, y: str | int, z: str | int | bytes):
    assert isinstance(x, str)
    assert isinstance(y, str) or isinstance(y, int)
    assert isinstance(z, str) or isinstance(z, int) or isinstance(z, bytes)
    assert isinstance(x, str) and isinstance(y, (str, int))
    assert not not isinstance(x, str)
    assert isinstance(x, str) and (isinstance(y, str) or isinstance(y, int))
    assert (isinstance(y, str) or isinstance(y, int)) and not not isinstance(x, str)
```

The ordinary rule still applies inside assertion tests. An assertion message computes a value, so
neither rule checks its `and` or `or` operands:

```py
def func(): ...
def assertion_boundaries(x: str, flag: bool):
    assert func and isinstance(x, str)  # error: [redundant-condition]
    assert flag, isinstance(x, str) and flag
```

The strict rule can still fire in assertion tests if the assertion test uses a walrus expression
(since tests that use walrus expressions are never flagged with `redundant-condition`, only ever
with `redundant-condition-strict`):

```py
# error: [redundant-condition-strict] "Object of type `Literal["foo"]` is always truthy"
assert (value := "foo")
```

## `sys.version_info` checks, `sys.platform` checks, `os.name` checks, `if TYPE_CHECKING` checks

Certain stdlib constants are heavily special-cased by ty, leading us to infer that certain `if`
tests involving these constants will always be truthy or always be falsy. Since the branches of code
here are deliberately unreachable, we try to avoid emitting false-positive diagnostics on these as
well:

`a.py`:

```py
import sys
import os
import typing
from typing import TYPE_CHECKING

def coinflip() -> bool:
    return False

reveal_type(sys.version_info >= (3, 14))  # revealed: Literal[True]
reveal_type(sys.version_info < (3, 15))  # revealed: Literal[True]

if sys.version_info >= (3, 14):  # no diagnostic
    pass

if coinflip():
    pass
elif sys.version_info < (3, 15):  # no diagnostic
    pass

if os.name == "posix":  # no diagnostic
    pass

if coinflip():
    pass
elif os.name == "nt":  # no diagnostic
    pass

reveal_type(TYPE_CHECKING)  # revealed: Literal[True]

if TYPE_CHECKING:  # no diagnostic
    pass

reveal_type(typing.TYPE_CHECKING)  # revealed: Literal[True]

if not typing.TYPE_CHECKING:  # no diagnostic
    pass

if sys.version_info < (3, 15):
    pass
elif (3, 12) <= sys.version_info < (3, 13):  # no diagnostic
    pass

if os.name == "posix":
    pass
elif os.name == "nt":  # no diagnostic
    pass
```

This also applies to the enabled-by-default `redundant-condition` rule, which only applies when
checking a condition that is not inferred as being assignable to `int`:

`b.py`:

```py
import sys

catch_exe_failure = "\n" if sys.platform == "win32" else ""

reveal_type(catch_exe_failure)  # revealed: Literal[""]

if catch_exe_failure:  # no diagnostic
    pass
```

This even applies to cases where the value of one of these constants is aliased to a variable in the
module namespace:

`c.py`:

```py
import os
import sys
from os import name as os_name
from typing import TYPE_CHECKING
from typing_extensions import TYPE_CHECKING as TYPE_CHECKINGGGGG
from sys import version_info as foo, platform as sys_platform

PLATFORM = sys.platform

if PLATFORM == "linux":  # no diagnostic
    pass

PLATFORM_ALIAS = PLATFORM

if PLATFORM_ALIAS == "linux":  # no diagnostic
    pass

OS_MODULE = os
OPERATING_SYSTEM = OS_MODULE.name

if OPERATING_SYSTEM == "posix":  # no diagnostic
    pass

IS_PY314 = sys.version_info >= (3, 14)
reveal_type(IS_PY314)  # revealed: Literal[True]

if IS_PY314:  # no diagnostic
    pass

if not IS_PY314:  # no diagnostic
    pass

VERSION_INFO = sys.version_info

if VERSION_INFO >= (3, 14):  # no diagnostic
    pass

CHECKING = TYPE_CHECKING

if CHECKING:  # no diagnostic
    pass

ORDINARY_CONSTANT = 1 == 1

if ORDINARY_CONSTANT:  # error: [redundant-condition-strict]
    pass

BAR = foo

reveal_type(BAR >= (3, 14))  # revealed: Literal[True]

if BAR >= (3, 14):  # no diagnostic
    pass

reveal_type(TYPE_CHECKINGGGGG)  # revealed: Literal[True]

if TYPE_CHECKINGGGGG:
    pass

reveal_type(sys_platform)  # revealed: Literal["linux"]

if sys_platform == "linux":  # no diagnostic
    pass

reveal_type(os_name)  # revealed: Literal["posix"]

if os_name == "posix":  # no diagnostic
    pass
```

And even in other imported modules:

`d.py`:

```py
import c
from c import IS_PY314, PLATFORM, BAR

if PLATFORM == "linux":  # no diagnostic
    pass

if c.PLATFORM_ALIAS == "linux":  # no diagnostic
    pass

if IS_PY314:  # no diagnostic
    pass

reveal_type(BAR >= (3, 14))  # revealed: Literal[True]

if BAR >= (3, 14):  # no diagnostic
    pass
```

Attribute aliases retain their environment-dependent origin. Different members of the same receiver
can have different origins, and rebinding or narrowing the receiver can change which definition an
attribute refers to.

`attribute_aliases.py`:

```py
import sys
from typing import Final

class PlatformConfig:
    enabled: Final = sys.platform == "linux"
    fixed: Final = True

class FixedConfig:
    enabled: Final = True

def rebound_receiver():
    config = PlatformConfig()
    if config.enabled:  # no diagnostic
        pass
    if config.fixed:  # error: [redundant-condition-strict] "Condition `config.fixed` is always true"
        pass

    config = FixedConfig()
    if config.enabled:  # error: [redundant-condition-strict] "Condition `config.enabled` is always true"
        pass

def narrowed_receiver(config: PlatformConfig | FixedConfig):
    if config.enabled:  # no diagnostic
        pass

    if isinstance(config, FixedConfig):
        if config.enabled:  # error: [redundant-condition-strict] "Condition `config.enabled` is always true"
            pass
    else:
        if config.enabled:  # no diagnostic
            pass
```

Named expressions and unpacked assignments preserve the same environment-dependent origin as
ordinary assignments. Their aliases remain exempt when tested later.

`assignment_forms.py`:

```py
import sys

if windows := sys.platform == "win32":  # no diagnostic
    pass
if windows:  # no diagnostic
    pass

unix, version = sys.platform != "win32", sys.version_info
if unix:  # no diagnostic
    pass
if version >= (3, 14):  # no diagnostic
    pass

def local_aliases():
    if is_windows := sys.platform == "win32":  # no diagnostic
        pass
    if is_windows:  # no diagnostic
        pass

    is_unix, major = sys.platform != "win32", sys.version_info.major
    if is_unix:  # no diagnostic
        pass
    if major >= 3:  # no diagnostic
        pass

if ordinary := 1 == 1:  # error: [redundant-condition-strict] "Condition `ordinary := 1 == 1` is always true"
    pass
if ordinary:  # error: [redundant-condition-strict] "Condition `ordinary` is always true"
    pass
```

Augmented assignments also preserve the environment-dependent origin of their right-hand side.

`augmented_assignment.py`:

```py
import sys

platform = ""
platform += sys.platform
if platform == "win32":  # no diagnostic
    pass

fixed = ""
fixed += "linux"
if fixed == "win32":  # error: [redundant-condition-strict] "Condition `fixed == "win32"` is always false"
    pass
```

Following aliases also terminates when assignments form a cycle. An ordinary cycle does not make an
always-truthy condition environment-dependent, whether the aliases are names or instance attributes.

`cyclic_aliases.py`:

```py
def plain_cycle(flag: bool):
    first = second = "ready"
    while flag:
        first = second
        second = first
    if first:  # error: [redundant-condition] "Object of type `Literal["ready"]` is always truthy"
        pass

class AttributeCycle:
    def check(self, flag: bool):
        self.first = self.second = "ready"
        while flag:
            self.first = self.second
            self.second = self.first
        if self.first:  # error: [redundant-condition] "Object of type `Literal["ready"]` is always truthy"
            pass
```

An environment-dependent assignment is still recognized after following a cycle of
instance-attribute aliases.

```py
import sys

class PlatformAttributeCycle:
    def check(self, flag: bool):
        self.first = self.second = "ready"
        while flag:
            self.first = self.second
            self.second = self.first
            self.second = sys.platform
        reveal_type(bool(self.first))  # revealed: Literal[True]
        if self.first:
            pass
```

## Environment-dependent loop targets

Loop targets inherit the environment-dependent origin of their iterable, including when the target
is unpacked or an alias is tested inside the loop.

```py
import sys

for is_windows in (sys.platform == "win32",):
    if is_windows:  # no diagnostic
        pass

for platform, version in ((sys.platform, sys.version_info),):
    alias = platform
    if alias == "win32":  # no diagnostic
        pass
    if version >= (3, 14):  # no diagnostic
        pass
```

Comprehension targets follow the same rule. The first iterable is evaluated in the enclosing scope;
later iterables are evaluated in the comprehension's scope.

```py
[flag for flag in (sys.platform == "win32",) if flag]  # no diagnostic
[flag for _ in range(1) for flag in (sys.platform == "win32",) if flag]  # no diagnostic
[flag for flag, _ in ((sys.platform == "win32", 0),) if flag]  # no diagnostic
```

Loop and comprehension targets without an environment-dependent source still produce diagnostics.

```py
for fixed in (True,):
    if fixed:  # error: [redundant-condition-strict] "Condition `fixed` is always true"
        pass

[fixed for fixed in (True,) if fixed]  # error: [redundant-condition-strict] "Condition `fixed` is always true"
```

## Environment-dependent pattern captures

Pattern captures inherit the environment-dependent origin of the match subject. This applies to
simple captures, unpacked captures, and aliases used in case guards.

```py
import sys

match sys.platform:
    case platform:
        if platform == "win32":  # no diagnostic
            pass

match (sys.platform, sys.version_info):
    case (platform, version):
        if platform == "win32":  # no diagnostic
            pass
        if version >= (3, 14):  # no diagnostic
            pass

match sys.platform == "win32":
    case is_windows if is_windows:  # no diagnostic
        pass
```

A capture of an ordinary constant is not exempt.

```py
match True:
    case fixed:
        if fixed:  # error: [redundant-condition-strict] "Condition `fixed` is always true"
            pass
```

## Environment-dependent context manager bindings

A `with` target can also inherit an environment-dependent value from its context expression.

```py
import sys
from contextlib import nullcontext

with nullcontext(sys.version_info) as version:
    if version >= (3, 14):  # no diagnostic
        pass

with nullcontext((1,)) as fixed:
    if fixed:  # error: [redundant-condition] "Object of type `tuple[int]` is always truthy"
        pass
```

## Environment references in called `lambda` functions and consumed generators

Calls can execute `lambda` bodies or consume generator expressions. Environment references inside
those bodies exempt the enclosing condition from both rules, including when the call's result is a
non-boolean object whose truthiness is known.

```py
import sys

if (lambda: sys.version_info >= (3, 12))():  # no diagnostic
    pass
if next(sys.platform == "linux" for _ in range(1)):  # no diagnostic
    pass

if (lambda: sys.platform)():  # no diagnostic
    pass
if next(sys.version_info for _ in range(1)):  # no diagnostic
    pass
```

The exemption also follows assignments and aliases, including when a named generator is consumed.

```py
platform = (lambda: sys.platform)()
if platform:  # no diagnostic
    pass

platforms = (sys.platform for _ in range(1))
alias = platforms
if next(alias):  # no diagnostic
    pass
```

## Deliberately exhaustive `if` statements

A common pattern is to have an `if` condition that is deliberately always true or false, so that the
user can assert exhaustiveness explicitly. We detect these cases and avoid emitting diagnostics on
them.

```py
import sys
from typing_extensions import assert_never

def f1(x: int | str):
    if isinstance(x, int):
        pass
    # always True, but no diagnostic emitted: the `else` block following only contains `raise` statements
    elif isinstance(x, str):
        pass
    else:
        raise AssertionError
        
def f2(x: int | str):
    if isinstance(x, int):
        pass
    # always False, but no diagnostic emitted: the block only contains `raise` statements
    elif not isinstance(x, str):
        raise AssertionError

def f3(x: int | str):
    if isinstance(x, int):
        pass
    # always True, but no diagnostic emitted: the `else` block following only contains `assert` statements
    elif isinstance(x, str):
        pass
    else:
        assert False, "unreachable"

def f4(x: int | str):
    if isinstance(x, int):
        pass
    # always True, but no diagnostic emitted: the `else` block following only contains calls that return `Never`
    elif isinstance(x, str):
        pass
    else:
        assert_never(x)

def f5(x: int | str):
    if isinstance(x, int):
        pass
    # always True, but no diagnostic emitted: the `else` block following only contains calls that return `Never`
    elif isinstance(x, str):
        pass
    else:
        "Some documentation as a standalone string, weirdly"
        sys.exit("This should never happen??")

def f6(x: int):
    # always True, but no diagnostic emitted: the block inside the `if` only contains `raise` statements
    if not isinstance(x, int):
        raise TypeError

def f7(x: int | str):
    if isinstance(x, int):
        pass
    # always True, but no diagnostic emitted: the `else` block following only contains `raise` statements
    elif isinstance(x, str) and not isinstance(x, int):
        pass
    else:
        raise AssertionError

def f8(x: int | str):
    if isinstance(x, int):
        pass
    # always False, but no diagnostic emitted: the block only contains `raise` statements
    elif not isinstance(x, str) or isinstance(x, int):
        raise AssertionError

def f9(x: str):
    # always False, but no diagnostic emitted: the block only contains `raise` statements
    if isinstance(x, str) and not isinstance(x, str):
        raise AssertionError

def f10(x: str):
    # always False, but no diagnostic emitted: the block only contains `raise` statements
    if not (isinstance(x, str) and isinstance(x, str)):
        raise TypeError

def coinflip() -> bool:
    return True

def f11(x: str):
    # always True, but no diagnostic emitted: every control flow path can be easily determined
    # to end in a terminal statement
    if not isinstance(x, str):
        if coinflip():
            message = "seems bad"
            raise TypeError(message)
        else:
            assert False, "oh no"
```

We also avoid emitting the diagnostic if the exhaustiveness check just follows the `if` check, and
is not in an `else` branch:

```py
def g(x: int | str):
    if isinstance(x, int):
        return

    # always True, but no diagnostic emitted: the code following only contains `raise` statements
    if isinstance(x, str):
        return

    raise AssertionError

def g2(x: int | str):
    if isinstance(x, int):
        return
    # always True, but no diagnostic emitted: the code following only contains `assert` statements
    elif isinstance(x, str):
        return
    
    assert False, "unreachable"
```

This also works if the entire block is nested:

```py
def unrelated_condition() -> bool:
    return False

def h(x: int | str):
    if unrelated_condition():
        if isinstance(x, int):
            return

        # always True, but no diagnostic emitted: the code following only contains `raise` statements
        if isinstance(x, str):
            return

        raise AssertionError
    # do other things that aren't raises or assertions:
    x = 1
```

An assertion that always succeeds does not establish exhaustiveness, whether it appears in the
conditional body, an `else` block, or immediately after the statement:

```py
def successful_assertion_in_body(value: int):
    if value is None:  # error: [redundant-condition-strict] "Condition `value is None` is always false"
        assert True

def successful_assertion_in_else(value: int):
    if value is not None:  # error: [redundant-condition-strict] "Condition `value is not None` is always true"
        pass
    else:
        assert True

def successful_assertion_after_if(value: int):
    if value is not None:  # error: [redundant-condition-strict] "Condition `value is not None` is always true"
        pass
    assert True
```

A nested conditional is only a defensive exit if its initial `if` body and every `elif` and `else`
body end in defensive exits. A body that falls through does not establish exhaustiveness.

```py
def nested_fallthrough(value: int, flag: bool):
    if value is None:  # error: [redundant-condition-strict] "Condition `value is None` is always false"
        if flag:
            print(value)
        else:
            raise AssertionError

def nested_without_else(value: int, flag: bool):
    if value is None:  # error: [redundant-condition-strict] "Condition `value is None` is always false"
        if flag:
            raise AssertionError
```

The first condition's type does not affect whether a later boolean condition is recognized as a
defensive check. Non-boolean conditions still produce the ordinary diagnostic, even when followed by
a defensive exit and the strict rule is enabled.

```py
def defensive_elif(items: list[int], value: int):
    if items:
        pass
    elif value is None:
        raise AssertionError

def predicate() -> bool:
    return False

def uncalled_function(flag: bool):
    if flag:
        pass
    elif predicate:  # error: [redundant-condition] "Function `predicate` is always truthy: Did you mean to call this function?"
        pass
    else:
        raise AssertionError
```

## Implicit `else` branches

An unrelated assertion after an `if` does not suppress a redundant-condition diagnostic when the
body ends in an ordinary call:

```py
def fallthrough(value: int, limit: int):
    if value is not None:  # error: [redundant-condition-strict] "Condition `value is not None` is always true"
        print(value)
    assert limit > 0
```

The same applies to a final `elif` whose body falls through:

```py
def fallthrough_elif(value: int | str):
    if isinstance(value, int):
        return
    elif isinstance(value, str):  # error: [redundant-condition-strict] "Condition `isinstance(value, str)` is always true"
        print(value)
    raise TypeError
```

We recognize an implicit `else` when the preceding body ends in a return, a raise, a call returning
`Never`, or a potentially failing assertion. A nested `if` must have an explicit `else`, and every
branch must end in one of these statements. These exits can be mixed within the nested conditional:

```py
from typing import Never

def stop() -> Never:
    raise RuntimeError

def nested_exits(value: int, choice: int, valid: bool):
    if value is not None:
        if choice == 0:
            return value
        elif choice == 1:
            raise ValueError
        elif choice == 2:
            stop()
        elif choice == 3:
            assert False
        else:
            assert valid
    raise TypeError
```

Potentially failing assertions count as exits even when they might succeed, because this heuristic
favors suppressing diagnostics on intentional runtime checks. An assertion that always succeeds does
not count as an exit:

```py
def successful_assertion(value: int):
    if value is not None:  # error: [redundant-condition-strict] "Condition `value is not None` is always true"
        assert True
    raise TypeError
```

A nested conditional that has a branch that falls through, or lacks an explicit `else`, does not
establish an implicit `else` after the outer `if`:

```py
def nested_fallthrough(value: int, flag: bool):
    if value is not None:  # error: [redundant-condition-strict] "Condition `value is not None` is always true"
        if flag:
            return value
        else:
            print(value)
    raise TypeError

def nested_without_else(value: int, flag: bool):
    if value is not None:  # error: [redundant-condition-strict] "Condition `value is not None` is always true"
        if flag:
            return value
    raise TypeError
```

An ordinary return in the implicit `else` is not a defensive exit, so it does not establish
exhaustiveness:

```py
def ordinary_return(value: int):
    if value is not None:  # error: [redundant-condition-strict] "Condition `value is not None` is always true"
        return value
    return 0
```

## Dunder methods that return `NotImplemented`

In dunder methods, it is usually more idiomatic to `return NotImplemented` rather than `raise` if
you're writing code with defensive runtime checks. We support this pattern too:

```py
class Foo:
    def __add__(self, other: "Foo") -> "Foo":
        # no diagnostic, even though this is inferred as always `True`!
        if not isinstance(other, Foo):
            return NotImplemented
        return self
```

## Tests that include walrus expressions

Walrus expressions always have side effects, so an always-true walrus expression may not always be
redundant. Examples of this can be found in CPython's scripts, where deliberately true walrus
expressions are used to continue the boolean-expression chain:

- <https://github.com/python/cpython/blob/f74cdf80a120649e4c353430da8cbd1305c00993/Tools/peg_generator/pegen/grammar_parser.py#L152-L168>

It is arguably always possible to write this kind of code in a clearer, more obvious way, so we
still emit a diagnostic on code like this, even though it may be deliberate. However, we use the
`redundant-condition-strict` rule for these patterns, so that the rule that is enabled by default is
unopinionated:

```py
def coinflip1() -> bool:
    return True

def coinflip2() -> bool:
    return True

foo = ("foo",)

# the always-truthy item is a `tuple[Literal["bar"]]`,
# so this would normally trigger `redundant-condition`,
# but the presence of the walrus expression means we use
# the disabled-by-default error code.
if coinflip1() and (foo := ("bar",)) and coinflip2():  # error: [redundant-condition-strict]
    ...
```

Walruses in `lambda` defaults or eager comprehensions can run while the condition is evaluated.
These conditions also use the strict rule.

```py
def eager_walruses(items: list[int]):
    if ((lambda value=(saved := 1): value),):  # error: [redundant-condition-strict]
        pass
    if ([saved := item for item in items],):  # error: [redundant-condition-strict]
        pass
    if ({saved := item for item in items},):  # error: [redundant-condition-strict]
        pass
    if ({item: (saved := item) for item in items},):  # error: [redundant-condition-strict]
        pass
```

## Walrus expressions in called `lambda` functions and consumed generators

Calling a `lambda` or consuming a generator can evaluate a walrus in its body. The nonempty tuples
returned here are always truthy, but the assignments run when evaluating the conditions. These
conditions therefore use only the strict rule.

```py
if (lambda: (value := (1,)))():  # error: [redundant-condition-strict]
    pass
if next((value := (1,)) for _ in range(1)):  # error: [redundant-condition-strict]
    pass
if next((1,) for item in range(3) if (value := item > 0)):  # error: [redundant-condition-strict]
    pass
```
