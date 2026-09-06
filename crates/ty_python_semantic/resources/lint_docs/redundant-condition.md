## What it does

Detects boolean conditions where the condition can be statically inferred to be always true or
always false due to its inferred type or short-circuit evaluation.

This rule is enabled by default, and is deliberately not comprehensive. In order to avoid false
positives, it excludes conditions that meet any of these criteria:

- The boolean test is inferred as evaluating to `True` itself, `False` itself, or an exact integer
    such as `1` or `0`.
- The condition uses a walrus operator (`:=`). The assignment's side effect may be intentional, even
    when its result has fixed truthiness.

## Why is this bad?

A boolean condition that is always true or always false usually indicates a mistake in your code,
and can often lead to incorrect behavior. If an `if` condition is inferred as always false,
moreover, ty will infer all code within that `if` branch as being unreachable, and will not report
any diagnostics on code in that region.

## Examples

A common error that triggers this rule is to forget to call a function, for example:

```py
import random


def should_do_action() -> bool:
    return random.choice([True, False])


# oops! You forgot the parentheses here... this should have been `if should_do_action()`.
# Because it's not, this will always be `True`:
if should_do_action:  # error: [redundant-condition]
    print("Doing stuff...")
```

Another common mistake is to forget to `await` a coroutine:

```py
import random


async def should_do_async_action():
    return random.choice([True, False])


async def main():
    # oops! Forgot the await here... this should have been `if await should_do_async_action()`.
    # Because it's not, this will always be `True`:
    if should_do_async_action():  # error: [redundant-condition]
        print("Doing stuff async...")
```

Or to forget that `tuple[X]` means "A tuple with exactly one element" rather than "a tuple with an
arbitrary number of elements" (for which you'd use `tuple[X, ...]`):

```py
# you almost certainly meant to write `tuple[str, ...]` here rather than `tuple[str]`...
def consume_tuples(x: tuple[str]):
    # ...and that means that this later condition is inferred as always being True by ty:
    if x:  # error: [redundant-condition]
        print("Got a non-empty tuple")
```

Some Pythonistas fall into the trap of thinking that a generator expression will be falsy if it has
zero elements inside it -- but generator expressions are lazy, and so they're always truthy unless
you collect them into a tuple:

```py
def test_my_data(data: list[int]):
    # this will always be `True`, because the asserted object is a `types.GeneratorType` instance,
    # not a `tuple`! `assert any(item for item in data if item > 42)`
    # is probably what you meant instead.
    assert (item for item in data if item > 42)  # error: [redundant-condition]
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

    if value < 1 < 0:  # error: [redundant-condition] "always false"
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

## Boolean operators used to compute values

The rule checks `and` and `or` operands when the expression is used as a condition: in an `if`,
`elif`, `while`, or `assert` test, a conditional expression, a comprehension filter, a match guard,
or as the operand of `not`. It does not flag `and` or `or` expressions used to compute values --
even if an operand in an `and` or `or` expression is always truthy, it doesn't necessarily make the
expression redundant:

```py
def f(): ...
def g(): ...


def test(coinflip: bool):
    # could also be written as `func = f if coinflip else g`,
    # but use of an `and` expression for this is common in older codebases.
    func = coinflip and f or g

    # `func` will be the `f` function if `coinflip` is `True`,
    # and the `g` function otherwise
    func()
```

This also allows calls that are deliberately always falsy but are used for their side effects:

```py
from unittest.mock import patch


def ask_to_continue() -> bool:
    return input("Continue? ") == "yes"


def test_ask_to_continue():
    prompts = []
    with patch(
        "builtins.input",
        side_effect=lambda prompt: prompts.append(prompt) or "yes",
    ):
        assert ask_to_continue()

    assert prompts == ["Continue? "]
```

By contrast, `not` always produces a boolean, so we will still emit a diagnostic on the following
example -- negating the truthiness of a function object is pointless, since a function object is
always truthy:

```py
def f(): ...


value = not f  # error: [redundant-condition]
```

## Known issues and workarounds

This rule can sometimes trigger on code that is not incorrect, but could be written in a clearer
way. For example, the rule will flag this code:

```py
def find_duplicate_coordinates(coordinates: list[tuple[int, int]]):
    seen: set[tuple[int, int]] = set()
    # error: [redundant-condition] "Expression `seen.add(coord)` is always falsy (has type `None`)"
    duplicates = {coord for coord in coordinates if coord in seen or seen.add(coord)}
    print(f"Duplicates are {duplicates}")
```

The error here is triggered due to `seen.add(coord)` being used in a boolean expression, despite the
fact that `set.add()` always returns `None`. Here this is deliberate: `set.add()` is being used for
its side effect.

To workaround this issue, the above code could be rewritten like this, which may also be easier for
some readers to understand:

```py
def find_duplicate_coordinates(coordinates: list[tuple[int, int]]):
    seen: set[tuple[int, int]] = set()
    duplicates: set[tuple[int, int]] = set()

    for coord in coordinates:
        if coord in seen:
            duplicates.add(coord)
        else:
            seen.add(coord)

    print(f"Duplicates are {duplicates}")
```
