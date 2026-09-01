# Short-Circuit Evaluation

## Not all boolean expressions must be evaluated

In `or` expressions, if the left-hand side is truthy, the right-hand side is not evaluated.
Similarly, in `and` expressions, if the left-hand side is falsy, the right-hand side is not
evaluated.

```py
def _(flag1: bool, flag2: bool):
    if flag1:
        pass
    elif flag2 or (x := 1):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[1]

def _(flag1: bool):
    if flag1 or (x := 1):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[1]

def _(flag1: bool, flag2: bool):
    if flag1:
        pass
    elif flag2 and (x := 1):
        reveal_type(x)  # revealed: Literal[1]

def _(flag1: bool):
    if flag1 and (x := 1):
        reveal_type(x)  # revealed: Literal[1]

def _(flag1: bool, flag2: bool):
    if flag1 and flag2 and (multi := 1):
        reveal_type(multi)  # revealed: Literal[1]

    if flag1 or (else_or := flag2):
        pass
    else:
        reveal_type(else_or)  # revealed: Literal[False]
```

## TODO: while loops

We currently use the precise truthy and falsy snapshots from boolean operators in `if` statements,
but not yet in `while` loops. These diagnostics should be removed when the same logic is applied to
`while` loop bodies and exits.

```py
def returns_bool() -> bool:
    return False

def _(flag: bool):
    while flag and (x := 1):
        reveal_type(x)  # revealed: Literal[1]

def _(flag: bool):
    while flag and (x := returns_bool()):
        reveal_type(x)  # revealed: Literal[True]

def _(flag: bool):
    while flag or (x := returns_bool()):
        pass
    # TODO: should not emit [possibly-unresolved-reference]
    # error: [possibly-unresolved-reference]
    reveal_type(x)  # revealed: Literal[False]

def _(flag: bool):
    while flag or (x := returns_bool()):
        pass
    else:
        # TODO: should not emit [possibly-unresolved-reference]
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[False]
```

## First expression is always evaluated

```py
def _(flag: bool):
    if (x := 1) or flag:
        reveal_type(x)  # revealed: Literal[1]

    if (x := 1) and flag:
        reveal_type(x)  # revealed: Literal[1]
```

## Statically known truthiness

```py
if True or (x := 1):
    # error: [unresolved-reference]
    reveal_type(x)  # revealed: Unknown

if True and (x := 1):
    reveal_type(x)  # revealed: Literal[1]
```

## Later expressions can always use variables from earlier expressions

```py
def _(flag: bool):
    flag or (x := 1) or reveal_type(x)  # revealed: Never

    # error: [unresolved-reference]
    flag or reveal_type(y) or (y := 1)  # revealed: Unknown
```

## Nested expressions

```py
def _(flag1: bool, flag2: bool):
    if flag1 or ((x := 1) and flag2):
        # error: [possibly-unresolved-reference]
        reveal_type(x)  # revealed: Literal[1]

    if ((y := 1) and flag1) or flag2:
        reveal_type(y)  # revealed: Literal[1]

    # error: [possibly-unresolved-reference]
    if (flag1 and (z := 1)) or reveal_type(z):  # revealed: Literal[1]
        # error: [possibly-unresolved-reference]
        reveal_type(z)  # revealed: Literal[1]
```

## Nested short-circuit assignments

Assignments in mutually exclusive short-circuit paths can still leave a name definitely bound.

```py
def _(flag: bool):
    if (flag and (x := 54)) or (x := 32):
        reveal_type(x)  # revealed: Literal[54, 32]

def _(flag: bool):
    (flag and (x := 1)) or (x := 2)
    reveal_type(x)  # revealed: Literal[1, 2]

def _(flag: bool, possibly_falsy_int: int, possibly_falsy_str: str):
    (flag and (x := possibly_falsy_int)) or (x := possibly_falsy_str)
    reveal_type(x)  # revealed: (int & ~AlwaysFalsy) | str

def _(flag: bool):
    (flag or (x := 0)) and (x := 2)
    reveal_type(x)  # revealed: Literal[0, 2]

def _(flag1: bool, flag2: bool):
    if (flag1 and (x := 1)) or (flag2 and (x := 2)):
        reveal_type(x)  # revealed: Literal[1, 2]

    if (flag1 or (y := 0)) and (flag2 or (y := 0)):
        pass
    else:
        reveal_type(y)  # revealed: Literal[0]

def _(flag1: bool, flag2: bool):
    (flag1 and (x := 1)) or (flag2 and (x := 2)) or (x := 3)
    reveal_type(x)  # revealed: Literal[1, 2, 3]

def _(flag1: bool):
    if (flag1 and (y := 1)) or (z := 2):
        # error: [possibly-unresolved-reference]
        reveal_type(y)  # revealed: Literal[1]
        # error: [possibly-unresolved-reference]
        reveal_type(z)  # revealed: Literal[2]
```

## Negated expressions

```py
def _(x: str):
    if not (x and (y := x)):
        raise ValueError

    reveal_type(y)  # revealed: str & ~AlwaysFalsy
```

## Other condition consumers

```py
def assert_statement(flag: bool):
    assert flag and (x := 1)
    reveal_type(x)  # revealed: Literal[1]

def if_expression(flag: bool):
    reveal_type(x) if flag and (x := 1) else None  # revealed: Literal[1]

def match_guard(flag: bool, subject: object):
    match subject:
        case _ if flag and (x := 1):
            reveal_type(x)  # revealed: Literal[1]

def comprehension_filter(flag: bool):
    [reveal_type(x) for _ in range(1) if flag and (x := 1)]  # revealed: Literal[1]
```

## Reachability of compound conditions

An `and` condition with an always-falsy operand cannot ever take the truthy branch. Similarly, an
`or` condition with an always-truthy operand cannot ever take the falsy branch.

This perhaps seems obvious, but it's not! Given the expression `value and False`, `value` could be
some object whose `__bool__` can return `False` on one call and `True` on the next. The evaluation
of `value and False` tests `value`, gets `False`, and short-circuits, meaning the entire expression
`value and False` evaluates to `value`. Now if we re-check truthiness of `value`, we can't
necessarily assume we get `False` again.

For code which saves the `and` expression to a variable, this is correct, and we do model this
possibility:

```py
def saved_condition(value: object):
    saved = value and False

    # We know that `saved` is not always truthy; we don't know that it's always falsy.
    reveal_type(saved)  # revealed: ~AlwaysTruthy

    if saved:
        # So this branch is reachable:
        reveal_type(value)  # revealed: object
```

But if the condition is tested directly, it works differently (at least in CPython). A short-circuit
within a branch condition doesn't just short-circuit to an evaluation of the expression; it
short-circuits directly to a control-flow decision, bypassing a final evaluation of the entire
condition expression, and avoiding the need for a second `__bool__` check. We model this
distinction:

```py
def conditions(value: object):
    if value and False:
        # This branch is not reachable; `value.__bool__` is only tested once. If it's false, this
        # branch is skipped immediately, if it's true, `False` is always false and this branch is
        # still skipped.
        reveal_type(value)  # revealed: Never

    if value or True:
        pass
    else:
        reveal_type(value)  # revealed: Never

    if not (value and False):
        pass
    else:
        reveal_type(value)  # revealed: Never

    if (value and False) or not (value or True):
        reveal_type(value)  # revealed: Never
```

Short-circuiting also skips later operands within a condition, including after nested boolean
operations.

```py
def nested_operands(value: object):
    if (value and False) and reveal_type(value):  # revealed: Never
        pass

    if (value or True) or reveal_type(value):  # revealed: Never
        pass

    if not (value or True) and reveal_type(value):  # revealed: Never
        pass
```

The same short-circuit rules apply to loop conditions, assertions, conditional expressions,
comprehension filters, and match guards.

```py
def other_conditions(value: object):
    while value and False:
        reveal_type(value)  # revealed: Never

    assert value or True, reveal_type(value)  # revealed: Never

    reveal_type(value) if value and False else None  # revealed: Never

    [reveal_type(item) for item in range(1) if value and False]  # revealed: Never

    match value:
        case _ if value and False:
            reveal_type(value)  # revealed: Never

    assert value and False
    reveal_type(value)  # revealed: Never
```

## Conditions with impossible operands

Narrowing can make a later operand impossible to evaluate. A `bool` cannot also be a `str`, so
`value` has type `Never` when it is tested again in each condition below. Only the earlier
short-circuit path can complete: falsy for `and`, truthy for `or`. We reveal an unrelated `marker`
to check that the whole branch is unreachable, independently of narrowing `value` itself.

```py
def impossible_operands(value: bool, marker: int):
    if isinstance(value, str) and value:
        reveal_type(marker)  # revealed: Never

    if not isinstance(value, str) or value:
        pass
    else:
        reveal_type(marker)  # revealed: Never

    if isinstance(value, str) and not value:
        reveal_type(marker)  # revealed: Never
```

These outcomes are preserved inside larger conditions, even when another operand has mutable
truthiness.

```py
def nested_impossible_operands(other: object, value: bool, marker: int):
    if other and (isinstance(value, str) and value):
        reveal_type(marker)  # revealed: Never

    if other or (not isinstance(value, str) or value):
        pass
    else:
        reveal_type(marker)  # revealed: Never
```

## Conditions with aliased `Never` operands

A call cannot produce a result when its return type is an alias of `Never`. Only the preceding
short-circuit path can complete.

```toml
[environment]
python-version = "3.12"
```

```py
from typing import Never

type Bottom = Never

def stop() -> Bottom:
    raise RuntimeError

def aliased_operand(flag: bool, marker: int):
    if flag and stop():
        reveal_type(marker)  # revealed: Never

    if flag or stop():
        pass
    else:
        reveal_type(marker)  # revealed: Never
```

A union of aliases of `Never` still cannot produce a result.

```py
type OtherBottom = Never
type BottomUnion = Bottom | OtherBottom

def stop_union() -> BottomUnion:
    raise RuntimeError

def union_operand(flag: bool, marker: int):
    if flag and stop_union():
        reveal_type(marker)  # revealed: Never
```

## Conditional expressions used as conditions

When a conditional expression (an `if/else` expression) is itself a condition, its selected branch
is evaluated as a condition too. The unselected branch does not affect whether the condition is
truthy.

```py
def conditional_expressions(value: object, flag: bool):
    if (value and False) if flag else False:
        reveal_type(value)  # revealed: Never

    if True if flag else (value or True):
        pass
    else:
        reveal_type(value)  # revealed: Never

    if True if value and False else False:
        reveal_type(value)  # revealed: Never
```

A branch narrowed to `Never` cannot contribute a result. The other branch alone determines the
conditional expression's truthiness.

```py
def impossible_branches(value: bool, marker: int):
    if value if isinstance(value, str) else False:
        reveal_type(marker)  # revealed: Never

    if True if not isinstance(value, str) else value:
        pass
    else:
        reveal_type(marker)  # revealed: Never
```

## Chained comparison conditions

A comparison chain used as a condition is falsy if any comparison is always falsy, even if an
earlier comparison returns an arbitrary object.

```py
class Comparable:
    def __lt__(self, other: int) -> object:
        return object()

def comparisons(value: Comparable):
    if value < 1 < 0:
        reveal_type(value)  # revealed: Never

    if value < 1 < 0 < 1:
        reveal_type(value)  # revealed: Never

    if (value < 1 < 0) and reveal_type(value):  # revealed: Never
        pass

    if not (value < 1 < 0):
        pass
    else:
        reveal_type(value)  # revealed: Never
```

Saving the result of a comparison chain can cause a non-boolean comparison result to be tested
twice. Its truthiness can change between those tests, so the truthy branch remains reachable.
References to `value` in these branches retain its type; in unreachable code they would have type
`Never`.

```py
def saved_comparison(value: Comparable):
    result = value < 1 < 0
    if result:
        reveal_type(value)  # revealed: Comparable

    if result := value < 1 < 0:
        reveal_type(value)  # revealed: Comparable
```

An unreachable assignment does not affect the inferred type of a loop variable. Inferring `value`
and deciding whether its assignment is reachable depend on each other, but `1 < 0` still makes the
branch unreachable.

```py
def loop_condition(flag: bool):
    value = 0
    while flag:
        if value < 1 < 0:
            value = Comparable()
        reveal_type(value)  # revealed: Literal[0]
```

## Re-testing boolean expression results

Saving the result of `value and False` and then testing it can call `value.__bool__` twice. The
second call may return a different result, so the truthy branch remains reachable. Assignment
expressions and nested boolean operations in value contexts can also cause this extra test.

```py
class MutableTruthiness:
    truthy: bool = False

    def __bool__(self) -> bool:
        self.truthy = not self.truthy
        return self.truthy

def expressions(value: MutableTruthiness, flag: bool):
    saved = value and False
    if saved:
        reveal_type(value)  # revealed: MutableTruthiness

    if saved := value and False:
        reveal_type(value)  # revealed: MutableTruthiness & ~AlwaysFalsy

    saved = (value and False) if flag else False
    if saved:
        reveal_type(value)  # revealed: MutableTruthiness

    result = (value and False) and reveal_type(value)  # revealed: MutableTruthiness & ~AlwaysFalsy
    result = (not (value or True)) or reveal_type(value)  # revealed: MutableTruthiness
```

Call arguments are evaluated as values, even when the call is itself used as a condition. Nested
boolean operations in an argument can therefore re-test an intermediate result.

```py
def call_argument(value: MutableTruthiness):
    if bool((value and False) and reveal_type(value)):  # revealed: MutableTruthiness & ~AlwaysFalsy
        pass
```

An assignment expression evaluates its right-hand side as a value before testing the assigned
object. Nested boolean operations on that right-hand side can therefore re-test an intermediate
result, even when the assignment expression is a condition.

```py
def assignment_expression(value: MutableTruthiness, marker: int):
    if saved := (value and False) and reveal_type(marker):  # revealed: int
        pass
```

A comprehension's filters are conditions, but its element is evaluated as a value, even when the
comprehension itself controls a branch.

```py
def comprehension_element(value: MutableTruthiness, marker: int):
    if [
        (value and False) and reveal_type(marker)  # revealed: int
        for _ in range(1)
        if value or True
    ]:
        pass
```
