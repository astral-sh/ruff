## What it does

Detects boolean conditions where the condition can be statically inferred to be always true or
always false.

This rule is disabled by default. It exclusively covers cases that its sibling (enabled-by-default)
rule `redundant-condition` does not cover. These cases often flag real bugs in user code, but also
have a significantly higher rate of unavoidable false positives than other cases.

This rule reports redundant conditions that meet any of these criteria:

- The boolean test is inferred as evaluating to `True` itself, `False` itself, or an exact integer
    such as `1` or `0`.
- Short-circuit evaluation means the condition can be guaranteed to be always truthy or always falsy
    despite fixed truthiness not being guaranteed by the inferred type of the expression's value
    (see "Short-circuiting boolean conditions" below for an example).
- The condition uses a walrus operator (`:=`). The assignment's side effect may be intentional, even
    when its result has fixed truthiness.

## Why is this bad?

A boolean condition that is always true or always false usually indicates a mistake in your code,
and can often lead to incorrect behavior. If an `if` condition is inferred as always false,
moreover, ty will infer all code within that `if` branch as being unreachable, and will not report
any diagnostics on code in that region.

## Examples

A common error in Python code is to make the mistake of thinking that indexing into a `bytes` object
will get you an object of type `bytes`. But `bytes` work differently to `str`s in Python -- although
a string is a sequence of strings, a bytestring is a sequence of `int`s, so indexing into a `bytes`
object gives you an `int`. This rule can catch that error by alerting you to the fact that checking
whether a `bytes` object is unequal to an `int` will always evaluate to `True`:

```py
def validate_record(data: bytes) -> None:
    if data[0] != b"\x1e":  # error: [redundant-condition-strict]
        raise ValueError("Invalid record separator")
```

Another common mistake is to assume that annotating `**kwargs` with `dict[str, str]` describes the
dictionary containing the keyword arguments. In fact, a `**kwargs` annotation describes each
individual keyword argument, so this annotation says that every value is itself a dictionary.
Comparing one of those values with a string will therefore always evaluate to `False`:

```py
def trace(**kwargs: dict[str, str]) -> None:
    if kwargs.get("operation") == "task":  # error: [redundant-condition-strict]
        print("Tracing task")
```

## Short-circuiting boolean conditions

In some situations, ty can know that a condition will always be true, or it can know that a
condition will always be false, even when this is not guaranteed by the inferred type of that
condition. This is because of the way that Python short-circuits evaluation of conditions in the
context of `if` tests, `while` tests and `assert` statements.

Consider a class whose comparison method has an `object` return type:

```py
from typing_extensions import reveal_type


class Comparable:
    def __lt__(self, other: int) -> object: ...


def check(value: Comparable):
    reveal_type(value < 1 < 0)  # revealed: ~AlwaysTruthy

    if value < 1 < 0:  # error: [redundant-condition-strict] "always false"
        pass
```

Outside the context of an `if` test, the revealed type of the condition here is `~AlwaysTruthy`: in
other words, ty knows that this expression is not *always true*, but cannot guarantee that it is
definitely *always false*. It could be an object that is sometimes true and sometimes false -- for
example, a `list` (which is falsy when it is empty, and truthy otherwise).

Nonetheless, when `value < 1 < 0` is used directly as a condition, ty knows that the condition will
always be falsy and the `if` branch will never be taken. Python tests the truthiness of the object
returned by `Comparable.__lt__` once: if it is falsy, the condition fails immediately. If it is
truthy, Python evaluates `1 < 0`, which is false. There is no second truthiness test of the object
returned by `__lt__`.

If the chained comparison is saved as a variable first, its value can be the object returned by
`__lt__`, if that object was falsy when first tested. The `if result` statement then tests that
object's truthiness again. A user-defined `__bool__` method can return a different result on that
second call, so ty cannot guarantee that the saved value is still falsy, and no diagnostic is
emitted:

```py
def check_saved(value: Comparable):
    result = value < 1 < 0
    if result:  # no diagnostic
        pass
```

## Exemptions

Like `redundant-condition`, this rule checks `and` and `or` operands only when the expression is
used as a condition. Expressions that compute values, such as assignments and return values, are
exempt. The operand of `not` is checked even when its result is used as a value, subject to the
exemptions below.

A common pattern in Python code is to use defensive `assert`s to enforce behaviour at runtime, even
when the asserted condition can be inferred statically to be always true. This rule therefore
exempts all assertion tests or subexpressions that evaluate to a subtype of `int` or `bool`:

```py
def add_one(x: int) -> int:
    assert isinstance(x, int)  # no diagnostic
    return x + 1
```

The rule can however still trigger on walrus expressions in `assert` statements that evaluate to an
always-truthy value that is not a `bool` or `int`:

```py
def func() -> bool:
    return True


def test_func():
    assert (result := func) and result != func()  # error: [redundant-condition-strict]
```

As well as the `assert` exemptions, this rule also exempts always-false `if` or `elif` conditions
when their bodies end in a defensive check: a `raise`, an assertion that could fail, a call
returning `Never` (including an awaited call), or `return NotImplemented`:

```py
def add_two(x: int) -> int:
    if not isinstance(x, int):  # no diagnostic
        raise TypeError("need an int!!")
    return x + 2
```

And an exemption is applied for always-true `if` or `elif` statements that are followed by branches
which contain defensive checks:

```py
from typing_extensions import assert_never


def parse_data(data: int | str):
    if isinstance(data, int):
        print("got an int")
    elif isinstance(data, str):  # Always true, but no diagnostic, since
        # the `else` branch following this branch is always terminal.
        # (`assert_never` returns `Never`, indicating that it always raises an exception)
        print("got a str")
    else:
        assert_never(data)


def parse_data_early_return(data: int | str):
    if isinstance(data, int):
        print("got an int")
        return

    if isinstance(data, str):  # Always true, but no diagnostic, since
        # the suite following this branch is always terminal
        # (every control-flow path following this `if` statement ends in a `raise` statement)
        print("got a str")
        return

    raise AssertionError("unexpected data")
```

Any conditions involving `sys.version_info`, `sys.platform`, `os.name` or `typing.TYPE_CHECKING` are
exempted. The rule recursively follows the definitions of names and attributes across module
boundaries to determine if a name or attribute was indirectly defined in relation to one of these
highly special-cased symbols:

```toml
[environment]
python-version = "3.14"
python-platform = "linux"
```

```py
import os
import sys
from typing import TYPE_CHECKING

if sys.version_info >= (3, 14):  # inferred as always true here, but no diagnostic
    pass

if sys.platform == "win32":  # inferred as always false here, but no diagnostic
    pass

if os.name == "posix":  # inferred as always true here, but no diagnostic
    pass

if TYPE_CHECKING:  # inferred as always true, but no diagnostic
    pass
```

Some conditions involving literal integers and booleans in the AST are also exempted: there's no
reason why you'd use a condition like this unless it was intentional.

```py
if True:  # inferred as always true (obviously), but no diagnostic
    pass

if 0:
    pass  # inferred as always false, but no diagnostic
```

## Known issues and workarounds

This rule can often trigger on code that is not incorrect, but could be written in a clearer way.
For example, the rule will flag this code:

```py
from enum import Enum


class YesOrNo(Enum):
    YES = 1
    NO = 0


def say_yes_or_no(what_to_say: YesOrNo):
    if what_to_say == YesOrNo.YES:
        print("yes")
    elif what_to_say == YesOrNo.NO:  # error: [redundant-condition-strict]
        print("no")
```

This snippet could be written more clearly as this, which would not trigger the rule owing to the
exemptions described in the section above:

```py
def say_yes_or_no(what_to_say: YesOrNo):
    if what_to_say == YesOrNo.YES:
        print("yes")
    else:
        assert what_to_say == YesOrNo.NO
        print("no")
```

or the snippet could also be rewritten as this, which would also be fine according to the rule's
heuristics:

```py
from typing_extensions import assert_never


def say_yes_or_no(what_to_say: YesOrNo):
    if what_to_say == YesOrNo.YES:
        print("yes")
    elif what_to_say == YesOrNo.NO:
        print("no")
    else:
        assert_never(what_to_say)
```

In a similar vein, this rule can often flag `and` or `or` expressions that have operands which are
deliberately always truthy or deliberately always falsy, because the purpose of the operand is to
have some side effect occur. For example:

```py
import random
from typing import Literal


def want_to_go_fishing() -> bool:
    return random.choice([True, False])


def weather_report() -> Literal["rainy", "sunny", "cloudy"]:
    return random.choice(["rainy", "sunny", "cloudy"])


def have_fishing_supplies() -> bool:
    return random.choice([True, False])


def main():
    if (
        want_to_go_fishing()
        and (weather := weather_report())  # error: [redundant-condition-strict]
        and have_fishing_supplies()
    ):
        print(f"The weather is {weather}, let's go fishing")
```

The middle operand in the above `and` expression is always truthy. This might be deliberate, but
even if it is, the function would arguably be clearer if it were written like this instead:

```py
def main():
    if want_to_go_fishing():
        weather = weather_report()
        if have_fishing_supplies():
            print(f"The weather is {weather}, let's go fishing")
```

Lastly, the rule cannot reliably distinguish in all cases comparisons that are intentionally always
true/false from those that are unintentionally always true/false. The rule takes care to avoid
flagging code that uses `if TYPE_CHECKING`, `if sys.version_info < (X, Y)`, `if sys.platform == ...`
and `if os.name == ...`. But it cannot reliably determine that code like this was written the way it
was meant to:

```py
DEBUGGING = 0

if DEBUGGING:  # error: [redundant-condition-strict]
    print("Doing debugging stuff...")
```
