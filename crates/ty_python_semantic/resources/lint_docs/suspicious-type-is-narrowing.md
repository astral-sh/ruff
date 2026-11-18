## What it does

Checks for `TypeIs[T]` return annotations where `T` can overlap with the declared type of the
parameter being narrowed but is not [assignable to] it.

This is a sibling rule to `disjoint-type-is-narrowing`, which flags `TypeIs` functions where `T` is
disjoint from the declared type of the parameter being narrowed. Unlike those cases, which are
always useless, the cases caught by this rule can be sound and useful in some situations.
Nonetheless, their usefulness is rare; this rule can often help identify mistakes in your code.

## Why is this bad?

`TypeIs` allows you to define functions that help ty narrow the type of an object based on a runtime
check. For example:

```python
from typing_extensions import TypeIs


def is_string(value: object) -> TypeIs[str]:
    return isinstance(value, str)


def process(value: int | str) -> None:
    if is_string(value):
        print(value.upper())  # value is a str
    else:
        print(value.bit_length())  # value is an int
```

When a `TypeIs[T]` function returns `True`, ty combines its existing knowledge of the argument's
type with the knowledge that it is also of type `T`. When the function returns `False`, ty can
exclude `T` from the argument's possible types.

For the inputs the function accepts, `TypeIs[T]` promises to return `True` exactly when the argument
is a `T`. Checking that `T` is [assignable to] the parameter type helps catch mistakes in the choice
of `T`. A target that is too broad can be a sign that the annotation describes a different test from
the one the function performs.

## Examples

`bad.py`:

```python
from collections.abc import Sequence
from typing_extensions import TypeIs, TypeAlias

Selection: TypeAlias = int | list[int] | set[int]


# error: "`Sequence[int]` is not assignable to the declared parameter type `int | list[int] | set[int]`"
def is_collection(value: Selection) -> TypeIs[Sequence[int]]:
    return isinstance(value, (list, set))


def first_three(value: Selection) -> None:
    if is_collection(value):
        # this will raise an exception if `value` is a `set`,
        # but the `TypeIs[Sequence[int]]` annotation tells ty the value is a sequence
        print(value[:3])


first_three({1, 2, 3})  # 💥
```

Lists of integers match both `Selection` and `Sequence[int]`. Tuples of integers also match
`Sequence[int]`, which makes it too broad for `Selection`. The function returns `True` for sets even
though a set is outside the declared narrowed type.

Declare the types for which the function returns `True`. ty can then flag the slice on a set:

`good.py`:

```py
from typing_extensions import TypeIs, TypeAlias

Selection: TypeAlias = int | list[int] | set[int]


# no error: `list[int] | set[int]` is assignable to `int | list[int] | set[int]`
def is_collection(value: Selection) -> TypeIs[list[int] | set[int]]:
    return isinstance(value, (list, set))


def first_three(value: Selection) -> None:
    if is_collection(value):
        # with the corrected `TypeIs` function, ty can now detect the invalid
        # subscription and emit an appropriate error:
        #
        # error: [not-subscriptable] "Cannot subscript object of type `set[int]`"
        print(value[:3])


first_three({1, 2, 3})  # 💥
```

## Rule status

This rule has [warning severity] by default because violating the assignability restriction does not
necessarily make a narrowing function unsafe.

For example, the following function performs sound narrowing, but is still flagged by this rule:

<!-- fmt:off -->

```python
from typing_extensions import TypeIs


class Foo: ...
class Bar: ...
class Both(Foo, Bar): ...


# Although `Bar` is not assignable to `Foo`, an object can be an instance of both classes.
def is_bar(value: Foo) -> TypeIs[Bar]:  # error: "`Bar` is not assignable to the declared parameter type `Foo`"
    return isinstance(value, Bar)
```

<!-- fmt:on -->

## References

[Typing specification] rules for `TypeIs` functions.

## Rule history

- v0.0.1-alpha.11: First reported as part of `invalid-type-guard-definition`.
- v0.0.106: Split out from `invalid-type-guard-definition` as a standalone rule.

[assignable to]: https://typing.python.org/en/latest/spec/glossary.html#term-assignable
[typing specification]: https://typing.python.org/en/latest/spec/narrowing.html#typeis
[warning severity]: https://docs.astral.sh/ty/rules/#rule-levels
