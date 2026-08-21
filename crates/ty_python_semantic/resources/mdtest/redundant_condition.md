# Detection of boolean tests that are always truthy or always falsy

A common error in Python is to accidentally test truthiness of the wrong object; for example
`if func:` (which is always true) where `if func():` was intended, or `if coroutine():` where
`if await coroutine():` was intended. By default, ty alerts the user to these errors with the error
code `redundant-condition`, but only if the inferred type of the object is not assignable to `int`.
This heuristic catches the `if func` and `if coroutine()` cases, while avoiding false positives on
cases such as `if DEBUG:` where `DEBUG = 0` or `DEBUG = False` is a constant.

The remaining cases -- where the inferred type is assignable to `int` -- are covered by a separate,
stricter rule (`redundant-condition-strict`).

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

And testing a generator expression without executing it:

```py
def work(items: list[int]):
    filtered = (item for item in items if item < 42)
    if filtered:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: A generator is always truthy
 --> src/mdtest_snippet.py:7:8
  |
7 |     if filtered:  # snapshot: redundant-condition
  |        ^^^^^^^^ Inferred type is `GeneratorType[int, None, None]`
help: Did you mean to collect the generator into a tuple?
  |
6 |     filtered = (item for item in items if item < 42)
  -     if filtered:  # snapshot: redundant-condition
7 +     if tuple(filtered):  # snapshot: redundant-condition
8 |         pass
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
  --> src/mdtest_snippet.py:11:8
   |
11 |     if coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
10 | async def main():
   -     if coroutine():  # snapshot: redundant-condition
11 +     if await coroutine():  # snapshot: redundant-condition
12 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

And testing a tuple that is known to always be empty or non-empty:

```py
class Foo:
    def __init__(self):
        self.single_element_tuple: tuple[int] = (42,)
        self.two_element_tuple: tuple[int, int] = (423, 432)
        self.at_least_one_element: tuple[int, *tuple[int, ...]] = (42,)
        self.at_least_two_elements: tuple[int, int, *tuple[int, ...]] = (42, 42)
        self.no_elements: tuple[()] = ()

    def other_method(self):
        if self.single_element_tuple:  # snapshot: redundant-condition
            pass
        if self.two_element_tuple:  # snapshot: redundant-condition
            pass
        if self.at_least_one_element:  # snapshot: redundant-condition
            pass
        if self.at_least_two_elements:  # snapshot: redundant-condition
            pass
        if self.no_elements:  # snapshot: redundant-condition
            pass
```

```snapshot
warning[redundant-condition]: A 1-element tuple is always truthy
  --> src/mdtest_snippet.py:22:12
   |
22 |         if self.single_element_tuple:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `tuple[int]`


warning[redundant-condition]: A 2-element tuple is always truthy
  --> src/mdtest_snippet.py:24:12
   |
24 |         if self.two_element_tuple:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `tuple[int, int]`


warning[redundant-condition]: A tuple with >=1 element is always truthy
  --> src/mdtest_snippet.py:26:12
   |
26 |         if self.at_least_one_element:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `tuple[int, *tuple[int, ...]]`


warning[redundant-condition]: A tuple with >=2 elements is always truthy
  --> src/mdtest_snippet.py:28:12
   |
28 |         if self.at_least_two_elements:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `tuple[int, int, *tuple[int, ...]]`


warning[redundant-condition]: An empty tuple is always falsy
  --> src/mdtest_snippet.py:30:12
   |
30 |         if self.no_elements:  # snapshot: redundant-condition
   |            ^^^^^^^^^^^^^^^^ Inferred type is `tuple[()]`
```

And testing `None`:

```py
X = None

if X:  # snapshot: redundant-condition
    pass
```

```snapshot
warning[redundant-condition]: `None` is always falsy
  --> src/mdtest_snippet.py:34:4
   |
34 | if X:  # snapshot: redundant-condition
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
  --> src/mdtest_snippet.py:39:4
   |
39 | if x:  # snapshot: redundant-condition
   |    ^ Inferred type is `Literal["foo"]`


warning[redundant-condition]: An empty string is always falsy
  --> src/mdtest_snippet.py:42:4
   |
42 | if y:  # snapshot: redundant-condition
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
  --> src/mdtest_snippet.py:47:8
   |
47 |     if x:  # snapshot: redundant-condition
   |        ^ Inferred type is `Literal["a", "b"]`
```

## Other boolean contexts

Redundant conditions are not merely detected in `if` tests. They are also detected in unary `not`
operations, `while` loops, `if` expressions, `and` expressions, `or` expressions, `match` guards,
and in comprehension `if` tests.

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

b = func and coinflip()  # error: [redundant-condition]

if func and coinflip():  # error: [redundant-condition]
    pass

c = func or coinflip()  # error: [redundant-condition]

if func or coinflip():  # error: [redundant-condition]
    pass

[x for x in range(3) if func]  # error: [redundant-condition]

def function(flag: bool):
    if flag:
        pass
    elif func:  # error: [redundant-condition]
        pass

assert func  # error: [redundant-condition]

while func and coinflip():  # error: [redundant-condition]
    pass

while not (func and coinflip()):  # error: [redundant-condition]
    pass

def f(x: str | int):
    match x:
        case str() if func:  # error: [redundant-condition]
            pass

# N.B. this `while` statement must come last in the test snippet,
# as ty considers all code following it to be unreachable,
# and does not emit any diagnostics in unreachable code!
#
while func:  # error: [redundant-condition]
    pass
```

## Edge cases

A nonempty tuple subclass can still be falsy if it overrides `__bool__`:

```py
from typing import Literal

async def coroutine(): ...

class FalsyTuple(tuple[int, int]):
    def __bool__(self) -> Literal[False]:
        return False

def check_falsy_tuple(value: FalsyTuple):
    if value:  # error: [redundant-condition] "Object of type `FalsyTuple` is always falsy"
        pass
```

Calling an asynchronous function or a function with an always-truthy return value does not resolve
the redundant condition, so we do not offer autofixes in these cases that add calls to the function:

```py
async def inspect_async_function():
    if coroutine:  # snapshot: redundant-condition
        pass

def always_truthy() -> Literal[True]:
    return True

def inspect_truthy_function():
    if always_truthy:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Function `coroutine` is always truthy
  --> src/mdtest_snippet.py:13:8
   |
13 |     if coroutine:  # snapshot: redundant-condition
   |        ^^^^^^^^^


warning[redundant-condition]: Function `always_truthy` is always truthy
  --> src/mdtest_snippet.py:20:8
   |
20 |     if always_truthy:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^
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
  --> src/mdtest_snippet.py:23:8
   |
23 |     if coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:27:28
   |
27 |     return lambda: True if coroutine() else False  # snapshot: redundant-condition
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
  --> src/mdtest_snippet.py:29:44
   |
29 |     return [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
   |                                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
28 | async def inspect_comprehension_awaitable():
   -     return [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
29 +     return [value for value in range(1) if await coroutine()]  # snapshot: redundant-condition
30 |
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:32:44
   |
32 |     return (value for value in range(1) if coroutine())  # snapshot: redundant-condition
   |                                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
31 | def inspect_generator_awaitable():
   -     return (value for value in range(1) if coroutine())  # snapshot: redundant-condition
32 +     return (value for value in range(1) if await coroutine())  # snapshot: redundant-condition
33 | async def inspect_named_awaitable():
   |
note: This is an unsafe fix and may change runtime behavior
```

Assignment expressions need parentheses so the assignment still happens before awaiting its result:

```py
async def inspect_named_awaitable():
    if value := coroutine():  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:34:8
   |
34 |     if value := coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
33 | async def inspect_named_awaitable():
   -     if value := coroutine():  # snapshot: redundant-condition
34 +     if await (value := coroutine()):  # snapshot: redundant-condition
35 |         pass
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
  --> src/mdtest_snippet.py:44:8
   |
44 |     if -value:  # snapshot: redundant-condition
   |        ^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
43 | async def inspect_awaitable_operations(value: AwaitableOperations):
   -     if -value:  # snapshot: redundant-condition
44 +     if await (-value):  # snapshot: redundant-condition
45 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:47:8
   |
47 |     if value + value:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
46 |
   -     if value + value:  # snapshot: redundant-condition
47 +     if await (value + value):  # snapshot: redundant-condition
48 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

Conditional expressions also need parentheses so the selected branch is awaited:

```py
async def inspect_conditional_awaitable(flag: bool):
    if coroutine() if flag else coroutine():  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:50:8
   |
50 |     if coroutine() if flag else coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
49 | async def inspect_conditional_awaitable(flag: bool):
   -     if coroutine() if flag else coroutine():  # snapshot: redundant-condition
50 +     if await (coroutine() if flag else coroutine()):  # snapshot: redundant-condition
51 |         pass
   |
note: This is an unsafe fix and may change runtime behavior
```

An expression that has already been awaited needs parentheses before adding another `await`:

```py
from types import CoroutineType
from typing import Any

async def nested_coroutine() -> CoroutineType[Any, Any, bool]:
    return coroutine()

async def inspect_nested_awaitable():
    if await nested_coroutine():  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:59:8
   |
59 |     if await nested_coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
58 | async def inspect_nested_awaitable():
   -     if await nested_coroutine():  # snapshot: redundant-condition
59 +     if await (await nested_coroutine()):  # snapshot: redundant-condition
60 |         pass
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

class AnnotatedHolder:
    async def inspect(self):
        self.value: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
```

```snapshot
warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:66:38
   |
66 |     type Alias = Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
   |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:68:42
   |
68 |     class Generic[T: Annotated[int, 1 if coroutine() else 0]]:  # snapshot: redundant-condition
   |                                          ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:71:40
   |
71 |     def generic[T: Annotated[int, 1 if coroutine() else 0]]():  # snapshot: redundant-condition
   |                                        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:74:46
   |
74 |     type GenericAlias[T: Annotated[int, 1 if coroutine() else 0]] = list[T]  # snapshot: redundant-condition
   |                                              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:76:34
   |
76 |     class GenericBase[T](Base if coroutine() else Base):  # snapshot: redundant-condition
   |                                  ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:79:43
   |
79 |     def nested(value: Annotated[int, 1 if coroutine() else 0]):  # snapshot: redundant-condition
   |                                           ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:82:43
   |
82 |     def returned() -> Annotated[int, 1 if coroutine() else 0]:  # snapshot: redundant-condition
   |                                           ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:85:35
   |
85 |     variable: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
   |                                   ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:89:41
   |
89 |         self.value: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
   |                                         ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
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
  --> src/mdtest_snippet.py:91:34
   |
91 |     class NongenericBase(Base if coroutine() else Base):  # snapshot: redundant-condition
   |                                  ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
90 | async def inspect_allowed_definition_awaitables():
   -     class NongenericBase(Base if coroutine() else Base):  # snapshot: redundant-condition
91 +     class NongenericBase(Base if await coroutine() else Base):  # snapshot: redundant-condition
92 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:94:46
   |
94 |     def generic_default[T](value: int = 1 if coroutine() else 0):  # snapshot: redundant-condition
   |                                              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
93 |
   -     def generic_default[T](value: int = 1 if coroutine() else 0):  # snapshot: redundant-condition
94 +     def generic_default[T](value: int = 1 if await coroutine() else 0):  # snapshot: redundant-condition
95 |         pass
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
  --> src/mdtest_snippet.py:97:38
   |
97 |     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
   |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
96 | async def inspect_runtime_type_expressions():
   -     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
97 +     alias = list[Annotated[int, 1 if await coroutine() else 0]]  # snapshot: redundant-condition
98 |     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:98:23
   |
98 |     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
   |                       ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
97 |     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
   -     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
98 +     value: int = 1 if await coroutine() else 0  # snapshot: redundant-condition
99 | if coroutine():  # snapshot: redundant-condition
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
  --> src/mdtest_snippet.py:99:4
   |
99 | if coroutine():  # snapshot: redundant-condition
   |    ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
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

def truthy(flag: bool):
    if flag:
        pass
    elif True:  # snapshot: redundant-condition-strict
        pass

def falsy(flag: bool):
    if flag:
        pass
    elif False:  # snapshot: redundant-condition-strict
        pass
```

```snapshot
error[redundant-condition-strict]: Condition is always truthy
 --> src/mdtest_snippet.py:7:8
  |
7 |     if x:  # snapshot: redundant-condition-strict
  |        ^ Inferred type is `Literal[1, 2]`


error[redundant-condition-strict]: Condition is always true
  --> src/mdtest_snippet.py:13:10
   |
13 |     elif True:  # snapshot: redundant-condition-strict
   |          ^^^^ Inferred type is `Literal[True]`


error[redundant-condition-strict]: Condition is always false
  --> src/mdtest_snippet.py:19:10
   |
19 |     elif False:  # snapshot: redundant-condition-strict
   |          ^^^^^ Inferred type is `Literal[False]`
```

`redundant-condition-strict` is also emitted on negated conditions where the negated condition is
inferred as an instance of `bool`:

```py
def negated_conditions():
    if not False:  # error: [redundant-condition-strict] "Condition is always true"
        pass

    if not True:  # error: [redundant-condition-strict] "Condition is always false"
        pass

    if not 0:  # error: [redundant-condition-strict] "Condition is always true"
        pass

    if not 1:  # error: [redundant-condition-strict] "Condition is always false"
        pass

    if not not True:  # error: [redundant-condition-strict] "Condition is always true"
        pass

def negated_conditional_contexts(flag: bool):
    if flag:
        pass
    elif not False:  # error: [redundant-condition-strict] "Condition is always true"
        pass

    while not False:  # error: [redundant-condition-strict] "Condition is always true"
        break
```

To avoid two diagnostics being emitted on compound tests such as the following statements, we
suppress `redundant-condition-strict` on subexpressions of `if`-statement tests, `elif` tests and
`while` tests. Only a single diagnostic is emitted on each of these:

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

## Infinite `while` loops

We maintain a special case for `while` loops, since `while True:` and `while 1:` are common idioms
used to create infinite loops in Python code. Complaining that the conditions `True` and `1` are
"always truthy" in these contexts would obviously be absurd.

Note that these need to be tested in separate files, as ty infers all code after a `while True` or
`while 1` loop to be unreachable, and it does not emit any diagnostics in unreachable code!

`while_true.py`:

```py
while True:  # no error
    pass
```

`while_1.py`:

```py
while 1:
    pass  # no error
```

## Defensive assertions

Of the two rules, only `redundant-condition` is applied to tests in `assert` statements (and any
subexpressions within those tests). This is to prevent false positives on defensive assertions such
as the following, which are common in well written Python code:

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

The ordinary rule still applies inside assertion tests, and the strict rule still applies to
assertion messages:

```py
def func(): ...
def assertion_boundaries(x: str, flag: bool):
    assert func and isinstance(x, str)  # error: [redundant-condition]
    assert flag, isinstance(x, str) and flag  # error: [redundant-condition-strict]
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
```

This even applies to cases where the value of one of these constants is aliased to a variable in the
module namespace:

`b.py`:

```py
import os
import sys
from typing import TYPE_CHECKING

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
```

And even in other imported modules:

`c.py`:

```py
import b
from b import IS_PY314, PLATFORM

if PLATFORM == "linux":  # no diagnostic
    pass

if b.PLATFORM_ALIAS == "linux":  # no diagnostic
    pass

if IS_PY314:  # no diagnostic
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
```

We also avoid emitting the diagnostic if the exhaustiveness check just follows the if check, and is
not in an `else` branch:

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
def successful_assertion_in_body():
    if "":  # error: [redundant-condition] "An empty string is always falsy"
        assert True

def successful_assertion_in_else():
    if "truthy":  # error: [redundant-condition] "Object of type `Literal["truthy"]` is always truthy"
        pass
    else:
        assert True

def successful_assertion_after_if():
    if "truthy":  # error: [redundant-condition] "Object of type `Literal["truthy"]` is always truthy"
        pass
    assert True
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

## Suites containing only string literals

A standalone string following an `if` statement does not assert exhaustiveness:

```py
def trailing_string():
    if True:  # error: [redundant-condition-strict]
        pass

    "This does not assert exhaustiveness."
```

A standalone string in an `else` block does not assert exhaustiveness either:

```py
def else_string():
    if True:  # error: [redundant-condition-strict]
        pass
    else:
        "This does not assert exhaustiveness."
```
