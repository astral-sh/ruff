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

Simply calling an asynchronous function would not resolve the redundant condition: the function must
be called *and* awaited, so this is what the autofix suggests:

```py
async def inspect_async_function():
    if coroutine:  # snapshot: redundant-condition
        pass
```

```snapshot
warning[redundant-condition]: Function `coroutine` is always truthy
  --> src/mdtest_snippet.py:13:8
   |
13 |     if coroutine:  # snapshot: redundant-condition
   |        ^^^^^^^^^ Did you mean to await and call this function?
   |
12 | async def inspect_async_function():
   -     if coroutine:  # snapshot: redundant-condition
13 +     if await coroutine():  # snapshot: redundant-condition
14 |         pass
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
  --> src/mdtest_snippet.py:19:8
   |
19 |     if always_truthy:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^ Did you mean to call this function?
   |
18 | def inspect_truthy_function():
   -     if always_truthy:  # snapshot: redundant-condition
19 +     if always_truthy():  # snapshot: redundant-condition
20 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Function `always_truthy_coro` is always truthy
  --> src/mdtest_snippet.py:26:8
   |
26 |     if always_truthy_coro:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^^^^^ Did you mean to await and call this function?
   |
25 | async def foo():
   -     if always_truthy_coro:  # snapshot: redundant-condition
26 +     if await always_truthy_coro():  # snapshot: redundant-condition
27 |         pass
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
  --> src/mdtest_snippet.py:29:8
   |
29 |     if coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:33:28
   |
33 |     return lambda: True if coroutine() else False  # snapshot: redundant-condition
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
  --> src/mdtest_snippet.py:35:44
   |
35 |     return [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
   |                                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
34 | async def inspect_comprehension_awaitable():
   -     return [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
35 +     return [value for value in range(1) if await coroutine()]  # snapshot: redundant-condition
36 |
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:38:44
   |
38 |     return (value for value in range(1) if coroutine())  # snapshot: redundant-condition
   |                                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
37 | def inspect_generator_awaitable():
   -     return (value for value in range(1) if coroutine())  # snapshot: redundant-condition
38 +     return (value for value in range(1) if await coroutine())  # snapshot: redundant-condition
39 | async def inspect_named_awaitable():
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
  --> src/mdtest_snippet.py:40:8
   |
40 |     if value := coroutine():  # snapshot: redundant-condition-strict
   |        ^^^^^^^^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
39 | async def inspect_named_awaitable():
   -     if value := coroutine():  # snapshot: redundant-condition-strict
40 +     if await (value := coroutine()):  # snapshot: redundant-condition-strict
41 |         pass
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
  --> src/mdtest_snippet.py:50:8
   |
50 |     if -value:  # snapshot: redundant-condition
   |        ^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
49 | async def inspect_awaitable_operations(value: AwaitableOperations):
   -     if -value:  # snapshot: redundant-condition
50 +     if await (-value):  # snapshot: redundant-condition
51 |         pass
   |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:53:8
   |
53 |     if value + value:  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
52 |
   -     if value + value:  # snapshot: redundant-condition
53 +     if await (value + value):  # snapshot: redundant-condition
54 |         pass
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
  --> src/mdtest_snippet.py:56:8
   |
56 |     if coroutine() if flag else coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
   |
55 | async def inspect_conditional_awaitable(flag: bool):
   -     if coroutine() if flag else coroutine():  # snapshot: redundant-condition
56 +     if await (coroutine() if flag else coroutine()):  # snapshot: redundant-condition
57 |         pass
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
  --> src/mdtest_snippet.py:65:8
   |
65 |     if await nested_coroutine():  # snapshot: redundant-condition
   |        ^^^^^^^^^^^^^^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, bool]`
help: Did you mean to `await` this expression?
   |
64 | async def inspect_nested_awaitable():
   -     if await nested_coroutine():  # snapshot: redundant-condition
65 +     if await (await nested_coroutine()):  # snapshot: redundant-condition
66 |         pass
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
  --> src/mdtest_snippet.py:72:38
   |
72 |     type Alias = Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
   |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:74:42
   |
74 |     class Generic[T: Annotated[int, 1 if coroutine() else 0]]:  # snapshot: redundant-condition
   |                                          ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:77:40
   |
77 |     def generic[T: Annotated[int, 1 if coroutine() else 0]]():  # snapshot: redundant-condition
   |                                        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:80:46
   |
80 |     type GenericAlias[T: Annotated[int, 1 if coroutine() else 0]] = list[T]  # snapshot: redundant-condition
   |                                              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:82:34
   |
82 |     class GenericBase[T](Base if coroutine() else Base):  # snapshot: redundant-condition
   |                                  ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:85:43
   |
85 |     def nested(value: Annotated[int, 1 if coroutine() else 0]):  # snapshot: redundant-condition
   |                                           ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:88:43
   |
88 |     def returned() -> Annotated[int, 1 if coroutine() else 0]:  # snapshot: redundant-condition
   |                                           ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:91:35
   |
91 |     variable: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
   |                                   ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:93:72
   |
93 |     list_comprehension: Annotated[int, [value for value in range(1) if coroutine()]]  # snapshot: redundant-condition
   |                                                                        ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:94:71
   |
94 |     set_comprehension: Annotated[int, {value for value in range(1) if coroutine()}]  # snapshot: redundant-condition
   |                                                                       ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:95:79
   |
95 |     dict_comprehension: Annotated[int, {value: value for value in range(1) if coroutine()}]  # snapshot: redundant-condition
   |                                                                               ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
  --> src/mdtest_snippet.py:98:61
   |
98 |         value: Annotated[int, [item for item in range(1) if coroutine()]],  # snapshot: redundant-condition
   |                                                             ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:103:46
    |
103 |         int, [value for value in range(1) if coroutine()]  # snapshot: redundant-condition
    |                                              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:109:41
    |
109 |         self.value: Annotated[int, 1 if coroutine() else 0]  # snapshot: redundant-condition
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
   --> src/mdtest_snippet.py:111:60
    |
111 |     direct: Annotated[int, (value for value in range(1) if coroutine())]  # snapshot: redundant-condition
    |                                                            ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
110 | async def inspect_generator_annotations():
    -     direct: Annotated[int, (value for value in range(1) if coroutine())]  # snapshot: redundant-condition
111 +     direct: Annotated[int, (value for value in range(1) if await coroutine())]  # snapshot: redundant-condition
112 |     nested: Annotated[int, ([value for value in range(1) if coroutine()] for _ in range(1))]  # snapshot: redundant-condition
    |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:112:61
    |
112 |     nested: Annotated[int, ([value for value in range(1) if coroutine()] for _ in range(1))]  # snapshot: redundant-condition
    |                                                             ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
111 |     direct: Annotated[int, (value for value in range(1) if coroutine())]  # snapshot: redundant-condition
    -     nested: Annotated[int, ([value for value in range(1) if coroutine()] for _ in range(1))]  # snapshot: redundant-condition
112 +     nested: Annotated[int, ([value for value in range(1) if await coroutine()] for _ in range(1))]  # snapshot: redundant-condition
113 | async def inspect_allowed_definition_awaitables():
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
   --> src/mdtest_snippet.py:114:34
    |
114 |     class NongenericBase(Base if coroutine() else Base):  # snapshot: redundant-condition
    |                                  ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
113 | async def inspect_allowed_definition_awaitables():
    -     class NongenericBase(Base if coroutine() else Base):  # snapshot: redundant-condition
114 +     class NongenericBase(Base if await coroutine() else Base):  # snapshot: redundant-condition
115 |         pass
    |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:117:46
    |
117 |     def generic_default[T](value: int = 1 if coroutine() else 0):  # snapshot: redundant-condition
    |                                              ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
116 |
    -     def generic_default[T](value: int = 1 if coroutine() else 0):  # snapshot: redundant-condition
117 +     def generic_default[T](value: int = 1 if await coroutine() else 0):  # snapshot: redundant-condition
118 |         pass
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
   --> src/mdtest_snippet.py:120:38
    |
120 |     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
    |                                      ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
119 | async def inspect_runtime_type_expressions():
    -     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
120 +     alias = list[Annotated[int, 1 if await coroutine() else 0]]  # snapshot: redundant-condition
121 |     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
    |
note: This is an unsafe fix and may change runtime behavior


warning[redundant-condition]: Condition is always truthy
   --> src/mdtest_snippet.py:121:23
    |
121 |     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
    |                       ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
120 |     alias = list[Annotated[int, 1 if coroutine() else 0]]  # snapshot: redundant-condition
    -     value: int = 1 if coroutine() else 0  # snapshot: redundant-condition
121 +     value: int = 1 if await coroutine() else 0  # snapshot: redundant-condition
122 | async def inspect_compound_awaitable(flag: bool):
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
   --> src/mdtest_snippet.py:123:17
    |
123 |     if flag and coroutine():  # snapshot: redundant-condition
    |                 ^^^^^^^^^^^ Inferred type is `CoroutineType[Any, Any, Unknown]`
help: Did you mean to `await` this expression?
    |
122 | async def inspect_compound_awaitable(flag: bool):
    -     if flag and coroutine():  # snapshot: redundant-condition
123 +     if flag and await coroutine():  # snapshot: redundant-condition
124 |         pass
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
   --> src/mdtest_snippet.py:125:4
    |
125 | if coroutine():  # snapshot: redundant-condition
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
    elif 1 == 1:  # snapshot: redundant-condition-strict
        pass

def falsy(flag: bool):
    if flag:
        pass
    elif 1 == 0:  # snapshot: redundant-condition-strict
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
13 |     elif 1 == 1:  # snapshot: redundant-condition-strict
   |          ^^^^^^ Inferred type is `Literal[True]`
help: Replace this `elif` with an `else` branch that asserts the condition to be `True`


error[redundant-condition-strict]: Condition is always false
  --> src/mdtest_snippet.py:19:10
   |
19 |     elif 1 == 0:  # snapshot: redundant-condition-strict
   |          ^^^^^^ Inferred type is `Literal[False]`
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

## `if` conditions that use AST literal bools or ints

Some projects use literal `if False:` or `if 0:` in their source code, to mark a region that is
intentionally unreachable, but which could be enabled for debugging purposes. If we see an AST
literal used as a condition, rather than a place that is inferred as having a literal *type*, we
suppress the diagnostic: it is assumed that this region is deliberately unreachable.

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

if sys.version_info < (3, 15):
    pass
elif (3, 12) <= sys.version_info < (3, 13):  # no diagnostic
    pass

if os.name == "posix":
    pass
elif os.name == "nt":  # no diagnostic
    pass
```

This even applies to cases where the value of one of these constants is aliased to a variable in the
module namespace:

`b.py`:

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

`c.py`:

```py
import b
from b import IS_PY314, PLATFORM, BAR

if PLATFORM == "linux":  # no diagnostic
    pass

if b.PLATFORM_ALIAS == "linux":  # no diagnostic
    pass

if IS_PY314:  # no diagnostic
    pass

reveal_type(BAR >= (3, 14))  # revealed: Literal[True]

if BAR >= (3, 14):  # no diagnostic
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

## Tests that include walrus expressions

Walrus expressions can have side effects, so an always-true walrus expression may not always be
redundant. Examples of this can be found in CPython's scripts, where deliberately true walrus
expressions are used to continue the boolean-expression chain:
- <https://github.com/python/cpython/blob/f74cdf80a120649e4c353430da8cbd1305c00993/Tools/peg_generator/pegen/grammar_parser.py#L152-L168>
- <https://github.com/python/cpython/blob/f74cdf80a120649e4c353430da8cbd1305c00993/Tools/peg_generator/pegen/grammar_parser.py#L152-L168>

It is arguably always possible to write this kind of code in a clearer, more obvious way, so we
still emit a diagnostic on code like this, even though it may be deliberate. However, we use the
`redundant-condition-strict` rule for these patterns, so that the rule that is enabled by default
is unopinionated:

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