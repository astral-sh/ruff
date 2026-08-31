# Detection of boolean tests that are always truthy or always falsy

A common error in Python is to accidentally test truthiness of the wrong object: for example
`if func:` (which is always true) where `if func():` was intended, or `if coroutine():` where
`if await coroutine():` was intended. By default, ty alerts the user to these errors with the error
code `redundant-condition`, but only if the inferred type of the object is not assignable to `int`.
This heuristic catches the `if func` and `if coroutine()` cases, while avoiding false positives on
cases such as `if DEBUG:` where `DEBUG = 0` or `DEBUG = False` is a constant.

The remaining cases -- where the inferred type is assignable to `int`, or only short-circuit
evaluation makes the condition's truthiness fixed -- are covered by a separate, stricter rule
(`redundant-condition-strict`).

```toml
[environment]
python-version = "3.14"
python-platform = "linux"
```

## Basic cases

We catch testing a function without calling it:

```py
def func(): ...

if func:  # TODO: should error
    pass
```

And testing a method without calling it:

```py
class Foo:
    def bar(self) -> bool:
        return True

    def baz(self):
        if self.bar:  # TODO: should error
            pass
```

And testing a generator expression without executing it:

```py
def work(items: list[int]):
    filtered = (item for item in items if item < 42)
    if filtered:  # # TODO: should error
        pass
    assert filtered  # # TODO: should error
```

And testing an awaitable without awaiting it:

```py
async def coroutine(): ...
async def main():
    if coroutine():  # TODO: should error
        pass
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
        if self.two_element_tuple:  # TODO: should error
            pass
        if self.at_least_one_element:  # TODO: should error
            pass
        if self.at_least_two_elements:  # TODO: should error
            pass
        if self.no_elements:  # TODO: should error
            pass

        # TODO: should error
        assert self.at_least_one_element
        # TODO: should error
        assert self.at_least_two_elements
```

And testing `None`:

```py
X = None

if X:  # TODO: should error
    pass
```

And testing a string that is known to always be truthy or always be falsy:

```py
x = "foo"
y = ""

if x:  # TODO: should error
    pass

if y:  # TODO: should error
    pass
```

or even a union of strings that is known to always be truthy:

```py
from typing import Literal

def f(x: Literal["a", "b"]):
    if x:  # TODO: should error
        pass
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
    if never_empty:  # TODO: should error
        pass

    if also_never_empty:  # TODO: should error
        pass

    if sometimes_empty:  # no diagnostic
        pass

    if also_sometimes_empty:  # no diagnostic
        pass

    assert never_empty  # TODO: should error
    assert also_never_empty  # TODO: should error
    assert sometimes_empty  # no diagnostic
    assert also_sometimes_empty  # no diagnostic
```

and testing an object that is known to be always truthy due to it being `@final` and not defining
`__bool__` or `__len__`:

```py
from re import Pattern

def f(x: Pattern[str]):
    if x:  # TODO: should error
        pass
```

## Required keys established by narrowing

A `TypedDict` with no declared required keys can be empty. After a key-presence check establishes
that a key is present, the dictionary is always truthy, so a subsequent truthiness check is
redundant.

```py
from typing import TypedDict

class Record(TypedDict):
    pass

def check(value: Record):
    if "x" in value:
        if value:  # TODO: should error
            pass
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
    if choice:  # TODO: should error
        pass
```

## Other boolean contexts

Redundant conditions are not merely detected in `if`-statement tests. They are also detected in
unary `not` operations, `while` loops, `assert` statements, `if` expressions, `match` guards, and
comprehension `if` tests. When an `and` or `or` expression is used as a condition, each operand is
checked.

An `and` or `or` expression used to compute a value is exempt. The assignments to `b` and `c` below
therefore produce no diagnostic, while the corresponding `if` conditions do.

```py
def coinflip() -> bool:
    return True

def func(): ...

if not func:  # TODO: should error
    pass

if not not func:  # TODO: should error
    pass

a = True if func else False  # TODO: should error

if coinflip() if func else False:  # TODO: should error
    pass

b = func and coinflip()  # no diagnostic

if func and coinflip():  # TODO: should error
    pass

c = func or coinflip()  # no diagnostic

if func or coinflip():  # TODO: should error
    pass

[x for x in range(3) if func]  # TODO: should error

def function(flag: bool):
    if flag:
        pass
    elif func:  # TODO: should error
        pass

def _():
    assert func  # TODO: should error

def _():
    while func and coinflip():  # TODO: should error
        pass

def _():
    while not (func and coinflip()):  # TODO: should error
        pass

def f(x: str | int):
    match x:
        case str() if func:  # TODO: should error
            pass

def _():
    while func:  # TODO: should error
        pass
```

## Always truthy values appearing later in compound conditions

A subexpression in a compound condition can be inferred as always truthy or always falsy even if the
condition overall is inferred as having ambiguous truthiness. We still report these subexpressions:

```py
def func(): ...
def compound_statement_conditions(flag: bool, other: bool):
    if flag and func:  # TODO: should error
        pass

    if other:
        pass
    elif flag and func:  # TODO: should error
        pass

    while flag and func:  # TODO: should error
        break

    match flag:
        case bool() if flag and func:  # TODO: should error
            pass

def compound_expression_conditions(flag: bool):
    selected = True if flag and func else False  # TODO: should error
    filtered = [value for value in range(1) if flag and func]  # TODO: should error
    result = flag and func

def compound_assertion_condition(flag: bool):
    assert flag and func  # TODO: should error
```

## Chained comparison conditions

A comparison chain used directly as a condition is always false if any comparison is always false,
even when an earlier comparison returns an object with mutable truthiness. The condition below
always fails because `1 < 0` is false.

```py
class Comparable:
    def __lt__(self, other: int) -> object: ...

def direct_condition(value: Comparable):
    reveal_type(value < 1 < 0)  # revealed: ~AlwaysTruthy
    reveal_type(bool(value < 1 < 0))  # revealed: bool

    # Short-circuiting makes the direct condition always false, despite the standalone types above.
    if value < 1 < 0:  # TODO: should flag `value < 1 < 0`
        pass

def negated_condition(value: Comparable):
    reveal_type(not (value < 1 < 0))  # revealed: bool
    reveal_type(bool(not (value < 1 < 0)))  # revealed: bool

    # Short-circuiting makes the direct condition always true, despite the standalone types above.
    if not (value < 1 < 0):  # TODO: should flag `not (value < 1 < 0)`
        pass
```

An always-false condition is exempt when its body raises an exception, since this can be a
deliberate defensive check. This exemption also applies when the condition is always false because
of short-circuit evaluation.

```py
def defensive_condition(value: Comparable):
    if value < 1 < 0:  # no diagnostic
        raise ValueError
```

Saving the chain's result, or negating it outside a condition, can cause an intermediate object's
truthiness to be tested twice. Its truthiness can change between those tests, so neither test below
has fixed truthiness.

```py
def saved_condition(value: Comparable):
    saved = value < 1 < 0
    reveal_type(saved)  # revealed: ~AlwaysTruthy
    reveal_type(bool(saved))  # revealed: bool

    if saved:  # no diagnostic
        pass
    return not (value < 1 < 0)  # no diagnostic
```

## Conditional expressions used as conditions

Using `a if flag else b` as a condition tests the truthiness of `a` when `flag` is true, or `b`
otherwise. We report uncalled functions in either position, even when the complete condition has
ambiguous truthiness. Reporting an uncalled function in a subexpression suppresses a second
diagnostic on the complete condition.

```py
def ready() -> bool:
    return False

def uncalled_functions(flag: bool):
    if ready if flag else False:  # TODO: should flag `ready`
        pass
    if False if flag else ready:  # TODO: should flag `ready`
        pass
    if ready if flag else True:  # TODO: should flag `ready`
        pass
    assert ready if flag else False  # TODO: should flag `ready`
```

The `not` operator also tests truthiness, so we report the uncalled function in
`not (ready if flag else False)`. `not` expressions are flagged in all contexts, not just
`if`/`elif`/`while`/`assert` tests:

```py
def negated_expression(flag: bool) -> bool:
    return not (ready if flag else False)  # TODO: should flag `ready`
```

Passing a function as an argument does not test its truthiness. Here, `callable()` checks whether
`ready` or `None` can be called, so there is no redundant truthiness test of `ready`:

```py
def callable_check(flag: bool):
    if callable(ready if flag else None):  # no diagnostic
        pass
```

Boolean branches inside an assertion remain exempt, since the assertion can defend against
incorrectly typed runtime values. Outside assertions, an always-true conditional expression is
reported as a whole:

```py
def boolean_branches(value: int, flag: bool):
    assert isinstance(value, int) if flag else True  # no diagnostic

    # TODO: should flag `isinstance(value, int) if flag else True`
    if isinstance(value, int) if flag else True:
        pass
```

Both branches of this conditional expression are truthy when evaluated directly as conditions. Even
if `value` has mutable truthiness, `value or True` short-circuits directly to the loop body when
`value` is truthy and evaluates `True` otherwise.

```py
def conditional_expression(value: object, flag: bool):
    # TODO: should flag `True if flag else (value or True)`
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
    if value:  # TODO: should error
        pass
```

## Strict version

Our stricter `redundant-condition-strict` rule extends this logic to boolean and integer tests:

```py
from typing import Literal

def f(x: Literal[1, 2]):
    if x > 5:  # TODO: should error
        pass

    if x:  # TODO: should error
        pass

def g(flag: bool, some_bytes: bytes):
    if flag:
        pass
    elif some_bytes[0] == b"\x1e":  # TODO: should error
        pass

def falsy(flag: bool):
    if flag:
        pass
    elif "foo" == b"foo":  # TODO: should error
        pass
```

`redundant-condition-strict` is also emitted on negated conditions where the negated condition is
inferred as an instance of `bool`:

```py
def negated_conditions():
    if not 1 > 2:  # TODO: should error
        pass

    if not 1 < 2:  # TODO: should error
        pass

    if not 0 == 1:  # TODO: should error
        pass

    if not 1 == 1:  # TODO: should error
        pass

    if not not 1 == 1:  # TODO: should error
        pass

def negated_conditional_contexts(flag: bool):
    if flag:
        pass
    elif not 1 == 0:  # TODO: should error
        pass

    while not 1 == 0:  # TODO: should error
        break
```

Outside a statement condition, a `not` expression still tests its operand's truthiness. The strict
rule reports redundant boolean and integer operands in assignments and return expressions:

```py
def negated_boolean_assignment(value: str):
    result = not isinstance(value, str)  # TODO: should error

def negated_integer_return(value: Literal[1, 2]) -> bool:
    return not value  # TODO: should error
```

When the strict rule can report that a complete compound condition is always true or always false,
it reports that condition instead of its operands. Only a single diagnostic is emitted on each of
these:

```py
def compound_truthy(x: str):
    if isinstance(x, str) and isinstance(x, str):  # TODO: should error
        pass

    while isinstance(x, str) and isinstance(x, str):  # TODO: should error
        break

    match x:
        case str() if isinstance(x, str) and isinstance(x, str):  # TODO: should error
            pass
```

## Redundant boolean operands in ambiguous conditions

When a condition's outcome is unknown, the strict rule reports individual operands with fixed
truthiness. These checks do not affect the outcome: `value is not None` is always true given the
annotation, while `value is None` is always false. The result depends on `enabled` in either case:

```py
def check(value: int, enabled: bool):
    if enabled and value is not None:  # TODO: should flag `value is not None`
        print(value)
    if value is not None and enabled:  # TODO: should flag `value is not None`
        print(value)
    if enabled or value is None:  # TODO: should flag `value is None`
        print(value)
    if value is None or enabled:  # TODO: should flag `value is None`
        print(value)
```

The same operand checks apply to loops, match guards, conditional expressions, and comprehension
filters:

```py
def condition_contexts(value: int, enabled: bool):
    while enabled and value is not None:  # TODO: should flag `value is not None`
        break

    match value:
        # TODO: should flag `value is not None`
        case _ if enabled and value is not None:
            pass

    # TODO: should flag `value is not None`
    selected = value if enabled and value is not None else 0

    # TODO: should flag `item is not None`
    filtered = [item for item in range(3) if enabled and item is not None]
```

Nested conditions are reported at the largest expression with fixed truthiness. Negation does not
hide a redundant operand when the complete condition still has unknown truthiness:

```py
def nested(value: int, enabled: bool):
    # TODO: should flag `value is not None and isinstance(value, int)`
    if enabled and (value is not None and isinstance(value, int)):
        print(value)
    if not (enabled or value is None):  # TODO: should flag `value is None`
        print(value)
    # TODO: should flag `(enabled and value is not None) or True`
    if (enabled and value is not None) or True:
        print(value)
```

When separate operands are redundant, both are reported. An always-true operand later in an `and`
expression does not replace a diagnostic on an earlier operand:

```py
def separate_operands(value: int, text: str, enabled: bool):
    if (
        value is not None  # TODO: should flag `value is not None`
        and enabled
        and isinstance(text, str)  # TODO: should flag `isinstance(text, str)`
    ):
        print(value)
```

An operand can have fixed truthiness due to short-circuit evaluation, even when its value type does
not guarantee that truthiness:

```py
def short_circuit_operands(value: object, enabled: bool):
    if enabled and (value or True):  # TODO: should flag `value or True`
        pass
    if enabled or (value and False):  # TODO: should flag `value and False`
        pass
```

The strict rule also checks the body and `else` expression of a conditional expression used as a
condition. Here, `value is not None` is always true, even though the complete condition can be false
when it evaluates to `enabled`:

```py
def conditional_branch(value: int, select: bool, enabled: bool):
    # TODO: should flag `value is not None`
    if value is not None if select else enabled:
        print(value)
```

## Compound conditions with mixed value types

Reporting a subexpression under `redundant-condition` takes precedence over reporting the complete
condition under `redundant-condition-strict`. Negating the condition does not add a second
diagnostic for the same subexpression.

```py
def func(): ...
def mixed_operands(value: object):
    if func and False:  # TODO: should flag `func`
        pass

    if not (value or func):  # TODO: should flag `func`
        pass
```

When neither operand is reported, the strict rule can report a fixed outcome established by
short-circuit evaluation, even if the expression's value type has ambiguous truthiness.

```py
def short_circuit(value: object):
    reveal_type(value and False)  # revealed: ~AlwaysTruthy
    reveal_type(bool(value and False))  # revealed: bool

    # Short-circuiting means this body is never reached, despite the standalone types above.
    if value and False:  # TODO: should flag `value and False`
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
    if accepts(not func):  # TODO: should error
        pass
```

`lambda` bodies and comprehension filters have their own scopes. `lambda` defaults and a
comprehension's first iterable are evaluated in the enclosing scope. Each nested boolean test is
reported once in either case.

```py
def nested_scopes():
    if accepts(lambda: not func):  # TODO: should error
        pass
    if accepts(lambda value=not func: value):  # TODO: should error
        pass
    if accepts([item for item in (not func,)]):  # TODO: should error
        pass
    if accepts([item for item in range(2) if not func]):  # TODO: should error
        pass
```

Compound conditions in conditional expressions and comprehension filters also report the complete
condition once, rather than both the condition and its negated operand.

```py
def compound_expression_tests():
    selected = 1 if not not (1 == 1) else 0  # TODO: should flag `not not (1 == 1)`
    filtered = [
        item
        for item in range(2)
        # TODO: should flag `not not (1 == 1)`
        if not not (1 == 1)
    ]
```

Each branch of a conditional expression can contain its own boolean test. Both `not func`
expressions are redundant, regardless of which one runs:

```py
def selected_values(flag: bool):
    # TODO: should flag both uses of `func`
    selected = not func if flag else not func
```

## Redundant boolean tests in call arguments

Boolean tests in call arguments are independent of the enclosing condition's truthiness:

```py
def accepts(value: bool) -> bool:
    return value

def nested_boolean_test(value: int, enabled: bool):
    # TODO: should flag `value is None`
    if enabled and accepts(not (value is None)):
        pass
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
    assert func and isinstance(x, str)  # TODO: should error
    
    # no diagnostic: `and` is used as a value expression here, not as a condition.
    assert flag, isinstance(x, str) and flag
```

Boolean and short-circuit operands within assertions remain exempt when the complete assertion has
unknown truthiness. This includes boolean tests nested inside call arguments:

```py
def accepts(value: bool) -> bool:
    return value

def ambiguous_boolean_and(value: int, flag: bool):
    assert flag and value is not None  # no diagnostic

def ambiguous_boolean_or(value: int, flag: bool):
    assert flag or value is None  # no diagnostic

def ambiguous_short_circuit(other: object, flag: bool):
    assert flag and (other or True)  # no diagnostic

def nested_boolean_assertion(value: int, flag: bool):
    assert flag and accepts(not (value is None))  # no diagnostic
```

Short-circuit conditions remain exempt when they are the complete assertion, whether they always
succeed or always fail:

```py
def short_circuit_assertion(value: object):
    assert value or True  # no diagnostic
    assert value and False  # no diagnostic
```

The strict rule can still fire in assertion tests if the assertion test uses a walrus expression
(since tests that use walrus expressions are never flagged with `redundant-condition`, only ever
with `redundant-condition-strict`):

```py
# TODO: should error
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
checking a condition that is not inferred as being assignable to `int`. A value that depends on an
environment guard is exempt whether it is assigned using a conditional expression or an `if`
statement:

`b.py`:

```py
import sys

catch_exe_failure = "\n" if sys.platform == "win32" else ""

reveal_type(catch_exe_failure)  # revealed: Literal[""]

if catch_exe_failure:  # no diagnostic
    pass

if sys.platform == "win32":
    line_prefix = "\n"
else:
    line_prefix = ""

reveal_type(line_prefix)  # revealed: Literal[""]

if line_prefix:  # no diagnostic
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

if ORDINARY_CONSTANT:  # TODO: should error
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
from b import line_prefix
from c import IS_PY314, PLATFORM, BAR

if line_prefix:  # no diagnostic
    pass

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
    if config.fixed:  # TODO: should error
        pass

    config = FixedConfig()
    if config.enabled:  # TODO: should error
        pass

def narrowed_receiver(config: PlatformConfig | FixedConfig):
    if config.enabled:  # no diagnostic
        pass

    if isinstance(config, FixedConfig):
        if config.enabled:  # TODO: should error
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

if ordinary := 1 == 1:  # TODO: should error
    pass
if ordinary:  # TODO: should error
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
if fixed == "win32":  # TODO: should error
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
    if first:  # TODO: should error
        pass

class AttributeCycle:
    def check(self, flag: bool):
        self.first = self.second = "ready"
        while flag:
            self.first = self.second
            self.second = self.first
        if self.first:  # TODO: should error
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

## Environment-dependent assignment guards

An assignment can depend on nested conditions or aliases of environment guards. The assigned value
remains exempt when tested inside a function:

```py
import sys

WINDOWS = sys.platform == "win32"

def nested_guards(enabled: bool):
    if enabled:
        if WINDOWS:
            prefix = "\n"
        else:
            prefix = ""
        reveal_type(prefix)  # revealed: Literal[""]
        if prefix:  # no diagnostic
            pass
```

Boolean values assigned under compound environment guards are also exempt, although they would
otherwise be reported by the strict rule:

```py
import os
from typing import TYPE_CHECKING

if os.name == "posix" and TYPE_CHECKING:
    enabled = True
else:
    enabled = False

reveal_type(enabled)  # revealed: Literal[True]
if enabled:  # no diagnostic
    pass
```

Assignments in `match` cases depend on the subject being matched, just as assignments in an `if`
statement depend on its condition:

```py
match sys.platform:
    case "win32":
        marker = ">"
    case _:
        marker = ""

reveal_type(marker)  # revealed: Literal[""]
if marker:  # no diagnostic
    pass
```

Ordinary predicates do not exempt assignments. A predicate can itself refer to the variable being
assigned without making it environment-dependent:

```py
def ordinary_guard(flag: bool):
    if flag:
        value = "ready"
    else:
        value = "ready"
    if value:  # TODO: should error
        pass

def recursive_guard():
    value = "ready"
    if value:  # TODO: should error
        value = "still ready"
```

A completed environment-dependent branch or a call that merely reads an environment constant does
not make subsequent assignments environment-dependent:

```py
if sys.platform == "win32":
    pass

print(sys.platform)
fixed = "ready"
if fixed:  # TODO: should error
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
    if fixed:  # TODO: should error
        pass

[fixed for fixed in (True,) if fixed]  # TODO: should error
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
        if fixed:  # TODO: should error
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
    if fixed:  # TODO: should error
        pass
```

## Environment references in called lambdas and consumed generators

Calls can execute lambda bodies or consume generator expressions. Environment references inside
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
    if value is None:  # TODO: should error
        assert True

def successful_assertion_in_else(value: int):
    if value is not None:  # TODO: should error
        pass
    else:
        assert True

def successful_assertion_after_if(value: int):
    if value is not None:  # TODO: should error
        pass
    assert True
```

A nested conditional is only a defensive exit if its initial `if` body and every `elif` and `else`
body end in defensive exits. A body that falls through does not establish exhaustiveness.

```py
def nested_fallthrough(value: int, flag: bool):
    if value is None:  # TODO: should error
        if flag:
            print(value)
        else:
            raise AssertionError

def nested_without_else(value: int, flag: bool):
    if value is None:  # TODO: should error
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
    elif predicate:  # TODO: should error
        pass
    else:
        raise AssertionError
```

## Defensive operands in ambiguous conditions

Type annotations are not enforced at runtime, and not all users run type checkers on their code.
Defensive runtime type checks are therefore common in well-written Python code.

In these examples, a caller could pass `None` despite the `int` annotation. We report no diagnostic
on the redundant condition because it can help reject that input: `value is not None` would be false
and lead to the `else` branch, while `value is None` would be true and enter the raising body:

```py
def defensive_else(value: int, enabled: bool):
    # no diagnostic: `value is not None` is always true, but the `else` branch
    # contains a defensive exit.
    if enabled and value is not None:
        print(value)
    else:
        raise TypeError

def defensive_body(value: int, enabled: bool):
    # no diagnostic: `value is None` is always false, but the `if` branch
    # contains a defensive exit.
    if enabled or value is None:
        raise TypeError
```

Negation reverses which branch an operand's truthiness contributes to. Defensive exits following an
early return also exempt the condition from being reported by either rule:

```py
def negated_defensive_body(value: int, enabled: bool):
    if not (enabled and value is not None):  # no diagnostic
        raise TypeError

def defensive_fallthrough(value: int, enabled: bool):
    if enabled and value is not None:  # no diagnostic
        return value
    raise TypeError
```

A defensive exit does not exempt an operand whose opposite truthiness would contribute to taking the
other branch. For example, a false result for `value is not None` below would skip the `raise`
rather than reach it:

```py
def nondefensive_operand(value: int, enabled: bool):
    if enabled and value is not None:  # TODO: should flag `value is not None`
        raise TypeError

def negated_nondefensive_operand(value: int, enabled: bool):
    if not (enabled or value is None):  # TODO: should flag `value is None`
        raise TypeError
```

Tests inside call arguments are independent of the enclosing condition's branches, so they do not
inherit its defensive-exit exemption:

```py
def accepts(value: bool) -> bool:
    return value

def independent_test(value: int):
    if accepts(not (value is None)):  # TODO: should flag `value is None`
        raise TypeError
```

## Implicit `else` branches

When an `if` body exits and the `if` statement has no explicit `else` branch, the following
statements act as an implicit `else`. Defensive checks in these implicit `else` branches are
recognised in the same way as defensive checks in explicit `else` branches. Ordinary fallthrough,
however, does not establish an implicit `else`.

For example, an unrelated assertion after an `if` does not suppress a redundant-condition diagnostic
when the `if` body ends in an ordinary call:

```py
def fallthrough(value: int, limit: int):
    if value is not None:  # TODO: should flag `value is not None`
        print(value)
    assert limit > 0
```

The same applies to a final `elif` whose body falls through:

```py
def fallthrough_elif(value: int | str):
    if isinstance(value, int):
        return
    elif isinstance(value, str):  # TODO: should flag `isinstance(value, str)`
        print(value)
    raise TypeError
```

We recognize an implicit `else` when the preceding `if` or `elif` branch ends in a `return`, a
`raise`, a call returning `Never`, or a potentially failing assertion. A nested `if` must have an
explicit `else`, and every branch must end in one of these statements. These exits can be mixed
within the nested conditional:

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
prioritises minimising false positives over catching every possible error. An assertion that always
succeeds does not count as an exit:

```py
def successful_assertion(value: int):
    if value is not None:  # TODO: should flag `value is not None`
        assert True
    raise TypeError
```

A nested conditional that has a branch that falls through, or lacks an explicit `else`, does not
establish an implicit `else` after the outer `if`:

```py
def nested_fallthrough(value: int, flag: bool):
    if value is not None:  # TODO: should flag `value is not None`
        if flag:
            return value
        else:
            print(value)
    raise TypeError

def nested_without_else(value: int, flag: bool):
    if value is not None:  # TODO: should flag `value is not None`
        if flag:
            return value
    raise TypeError
```

An ordinary return in the implicit `else` is not a defensive exit, so it does not establish
exhaustiveness:

```py
def ordinary_return(value: int):
    if value is not None:  # TODO: should flag `value is not None`
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
if coinflip1() and (foo := ("bar",)) and coinflip2():  # TODO: should error
    ...
```

Walruses in lambda defaults or eager comprehensions can run while the condition is evaluated. These
conditions also use the strict rule.

```py
def eager_walruses(items: list[int]):
    if ((lambda value=(saved := 1): value),):  # TODO: should error
        pass
    if ([saved := item for item in items],):  # TODO: should error
        pass
    if ({saved := item for item in items},):  # TODO: should error
        pass
    if ({item: (saved := item) for item in items},):  # TODO: should error
        pass
```

## Walrus expressions in called lambdas and consumed generators

Calling a lambda or consuming a generator can evaluate a walrus in its body. The nonempty tuples
returned here are always truthy, but the assignments run when evaluating the conditions. These
conditions therefore use only the strict rule.

```py
if (lambda: (value := (1,)))():  # TODO: should error
    pass
if next((value := (1,)) for _ in range(1)):  # TODO: should error
    pass
if next((1,) for item in range(3) if (value := item > 0)):  # TODO: should error
    pass
```
