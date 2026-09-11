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

Like `redundant-condition`, this rule checks subexpressions of an `and` or `or` expression only when
the outer expression is used as a condition. This is to avoid emitting false-positive diagnostics on
code like the following, where the `and` expression is clearly not redundant despite the fact that
both `CONSTANT_1` and `CONSTANT_2` are always truthy:

```py
from typing import Final


CONSTANT_1: Final = 1
CONSTANT_2: Final = 2


def do_something(coinflip: bool):
    # could also be written as `constant_to_use = CONSTANT_1 if coinflip else CONSTANT_2`,
    # but use of an `and` expression for this is common in older codebases.
    constant_to_use = coinflip and CONSTANT_1 or CONSTANT_2

    # do something with `constant_to_use` now...
    ...
```

Unlike `and` and `or`, however, `not` explicitly converts its operand to a boolean, so the rule
checks `not` expressions in every context.

Another exemption applied by this rule concerns `assert`-statement tests. A common pattern in Python
code is to use defensive `assert`s to enforce behaviour at runtime, even when the asserted condition
can be inferred statically to be always true. For example:

```py
def add_one(x: int) -> int:
    assert isinstance(x, int)  # no diagnostic
    return x + 1
```

This kind of defensive behaviour is often reasonable, since the author of a library cannot guarantee
that end users of the library will run a type checker on code calling into the library, meaning that
it's entirely possible at runtime for an object passed into the `x`a parameter above to be a `str`
(for example) even though the parameter annotation states that only `int`s can ever be passed in.
This rule therefore also exempts all assertion tests or subexpressions that evaluate to a subtype of
`int` or `bool`:

`redundant-condition-strict` can still trigger on `assert` statements in some contexts, however. For
example, `redundant-condition-strict` will be emitted on the below example, where the left-hand side
of the `and` expression is always true and not a subtype of `bool` or `int`, but where the condition
is nonetheless excluded from the enabled-by-default `redundant-condition` rule due to the use of the
walrus operator:

```py
def func() -> bool:
    return True


def test_func():
    assert (result := func) and result != func()  # error: [redundant-condition-strict]
```

For similar reasons to the `assert` exemptions, this rule also exempts always-false `if` or `elif`
conditions when their bodies end in a defensive check: a `raise`, an assertion that could fail, a
call returning `Never`, an `await` to a call returning `Never`, or `return NotImplemented`:

```py
import sys


def add_two(x: int) -> int:
    if not isinstance(x, int):  # no diagnostic
        raise TypeError("need an int!!")
    return x + 2


def add_three(x: int) -> int:
    if not isinstance(x, int):  # no diagnostic
        assert False, "unreachable"
    return x + 3


def add_four(x: int) -> int:
    if not isinstance(x, int):  # no diagnostic
        sys.exit(1)
    return x + 4


class Foo:
    def __init__(self, data: int):
        self.data = data

    def __add__(self, other: "Foo"):
        if not isinstance(other, Foo):  # no diagnostic
            return NotImplemented
        return Foo(self.data + other.data)
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

    # Always true, but no diagnostic, since
    # the suite following this branch is always terminal
    # (every control-flow path following this `if` statement ends in a `raise` statement)
    if isinstance(data, str):
        print("got a str")
        return

    raise AssertionError("unexpected data")
```

Any conditions defined in relation to `sys.version_info`, `sys.platform`, `os.name` or
`typing.TYPE_CHECKING` are also exempted. The rule recursively follows the definitions of names and
attributes across module boundaries to determine if a name or attribute was indirectly defined in
relation to one of these highly special-cased symbols:

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

LINE_ENDING = "\n" if os.name == "posix" else "\r\n"

if LINE_ENDING == "\n":  # inferred as always true here, but no diagnostic
    pass

if TYPE_CHECKING:  # inferred as always true, but no diagnostic
    pass
```

Conditions involving these constants, or conditions involving values defined in relation to these
constants, can often be inferred as always-true or always-false by ty. Indeed, these conditions
usually *will* be always true or always false across a single invocation run of a Python programme.
Nonetheless, Python code is often written so that it can work on multiple different Python versions
and/or multiple different operating systems, and a condition that is always true on one operating
system might very well be always false on another operating system (for example). Flagging these
conditions as being always true or always false would only add noise: the aim of the rule is to flag
conditions that are *unintentionally* always true or always false.

Lastly, some conditions involving literal integers and booleans in the AST are also exempted:
there's no reason why you'd use a condition like this unless it was intentional.

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
was meant to be:

```py
DEBUGGING = 0

if DEBUGGING:  # error: [redundant-condition-strict]
    print("Doing debugging stuff...")
```
