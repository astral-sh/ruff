## What it does

Detects boolean conditions where the condition can be statically inferred to be always true or
always false due to the inferred type of the condition.

This rule is enabled by default, and is deliberately not comprehensive. In order to avoid false
positives, it excludes conditions that meet any of these criteria:

- The boolean test is inferred as evaluating to `True` itself, `False` itself, or an exact integer
    such as `1` or `0`.
- The boolean test can be inferred as always evaluating to `True` and `False`, but this inference is
    due to boolean-test short-circuiting in `if` conditions, `while` conditions or `assert` tests
    rather than the inferred type of the boolean test.
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

## Exemptions

### Boolean operators used to compute values

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

By contrast, `not` always produces a boolean, so we will still emit a diagnostic on the following
example -- negating the truthiness of a function object is pointless, since a function object is
always truthy:

```py
def f(): ...


value = not f  # error: [redundant-condition]
```

### Calls returning `None`

Calls returning `None` are often used for their side effects in conditional expressions and
comprehension filters. `redundant-condition` and `redundant-condition-strict` both therefore exempt
these calls when they contribute to an `and` or `or` expression in the test. This includes negated
calls, such as `item not in seen and not seen.add(item)`:

```py
def find_duplicate_coordinates(coordinates: list[tuple[int, int]]):
    seen: set[tuple[int, int]] = set()
    # No diagnostic here, even though `seen.add(coord)` returns `None`, which is always falsy
    duplicates = {coord for coord in coordinates if coord in seen or seen.add(coord)}
    print(f"Duplicates are {duplicates}")
```

Here, `seen.add(coord)` records each new coordinate while its `None` result excludes that coordinate
from the set of duplicates.

Calls used as the entire test are still reported. For example, this function attempts to label each
item as new or repeated. But `set.add` returns `None` regardless of whether the item was already in
the set, so the conditional expression always selects `"repeat"`:

```py
def label_items(items: list[str]) -> list[str]:
    seen: set[str] = set()
    return [
        "new" if seen.add(item) else "repeat"  # error: [redundant-condition]
        for item in items
    ]


assert label_items(["red", "blue", "red"]) == ["repeat", "repeat", "repeat"]
```

Using `set.add` as the entire comprehension filter cannot remove duplicates either. Its falsy return
value rejects every item, leaving an empty list:

```py
items = ["red", "blue", "red"]
seen: set[str] = set()

unique = [item for item in items if seen.add(item)]  # error: [redundant-condition]
assert unique == []
```

Negating the call instead admits every item, including duplicates. We report this filter too,
because it still has no effect on which items are included:

```py
items = ["red", "blue", "red"]
seen: set[str] = set()

unique = [item for item in items if not seen.add(item)]  # error: [redundant-condition]
assert unique == ["red", "blue", "red"]
```

A standalone `not` expression is exempt. For example, a logging filter can save each message for
inspection while allowing it to reach the logger's handlers. Returning `True` lets the message
through; `not` converts the `None` returned by `append` to that result:

```py
from logging import Logger, StreamHandler

messages: list[str] = []
logger = Logger("capture")
logger.addHandler(StreamHandler())
logger.addFilter(
    lambda record: not messages.append(record.getMessage())
)  # no diagnostic

logger.warning("Saved report")  # Emits "Saved report".
assert messages == ["Saved report"]
```

The same rules apply to awaited calls that produce `None`. This function queues each unfinished job
and returns the jobs it queued. `Queue.put` produces `None` when awaited, so negating that result
keeps each queued job in the returned list:

```py
from asyncio import Queue


async def enqueue_pending(
    jobs: list[str], completed: set[str], queue: Queue[str]
) -> list[str]:
    return [
        job
        for job in jobs
        if job not in completed and not await queue.put(job)  # no diagnostic
    ]
```

The exemption does not apply when a call returning `None` is nested inside an outer boolean test, or
when the call itself is a statement condition:

```py
def record() -> None: ...


def check(flag: bool, other_flag: bool):
    if record():  # error: [redundant-condition]
        pass

    if not record():  # error: [redundant-condition]
        pass

    if flag if record() else other_flag:  # error: [redundant-condition]
        pass
```
