# Comparison: Membership Test

In Python, "membership test operators" refer to `in` and `not in` operator. To customize their behavior, classes can implement methods like `__contains__`, `__iter__`, or `__getitem__`.

For references, see:

- <https://docs.python.org/3/reference/expressions.html#membership-test-details>
- <https://docs.python.org/3/reference/datamodel.html#object.__contains__>

## Implements `__contains__`

Classes can support membership tests by implementing the `__contains__` method:

```py
class A:
    def __contains__(self, item: str) -> bool: ...


reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
# TODO: should be Unknown, need to check arg type and should be failed
reveal_type(42 in A())  # revealed: bool
reveal_type(42 not in A())  # revealed: bool
```

## Implements `__iter__`

Classes that don't implement `__contains__`, but do implement `__iter__`, also support containment checks; the needle will be sought in their iterated items:

```py
class A:
    def __iter__(self) -> Iterator[str]: ...


reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
reveal_type(42 in A())  # revealed: bool
reveal_type(42 not in A())  # revealed: bool
```

## Implements `__getitems__`

The final fallback is to implement `__getitem__` for integer keys: Python will call it with 0, 1, 2... until it either finds the needle (returning True for the membership test) or `__getitem__` raises IndexError, which is silenced and returns `False` for the membership test.

```py
class A:
    def __getitem__(self, key: int) -> str: ...


reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
reveal_type(42 in A())  # revealed: bool
reveal_type(42 not in A())  # revealed: bool
```

## Wrong Return Type

Python coerces the results of containment checks to bool, even if `__contains__` returns a non-bool:

```py
class A:
    def __contains__(self, item: str) -> str: ...


reveal_type("hello" in A())  # revealed: bool
reveal_type("hello" not in A())  # revealed: bool
```

## Literal Result for in and not in Checks

Tests with Literals as return types in `__contains__`, the result of operator also should be BooleanLiteral:

```py
class AlwaysTrue:
    def __contains__(self, item: int) -> Literal[1]: ...


class AlwaysFalse:
    def __contains__(self, item: int) -> Literal[""]: ...


# TODO: it should be Literal[True] and Literal[False]
reveal_type(42 in AlwaysTrue())  # revealed: @Todo
reveal_type(42 not in AlwaysTrue())  # revealed: @Todo

# TODO: it should be Literal[False] and Literal[True]
reveal_type(42 in AlwaysFalse())  # revealed: @Todo
reveal_type(42 not in AlwaysFalse())  # revealed: @Todo
```

## No Fallback for `__contains__`

If `__contains__` is implemented, checking membership of a type it doesn't accept is an error; it doesn't result in a fallback to `__iter__` or `__getitem__`:

```py
class Contains: ...


class Iter: ...


class GetItem: ...


class A:
    def __contains__(self, item: Contains) -> bool: ...

    def __iter__(self) -> Iterator[Iter]: ...

    def __getitem__(self, key: int) -> GetItem: ...


reveal_type(Contains() in A())  # revealed: bool

# TODO: should be `Unknown`, need to check arg type, and should not fall back to __iter__ or __getitem__
reveal_type(Iter() in A())  # revealed: bool
reveal_type(GetItem() in A())  # revealed: bool


class B:
    def __iter__(self) -> Iterator[Iter]: ...

    def __getitem__(self, key: int) -> GetItem: ...


reveal_type(Iter() in B())  # revealed: bool
# GetItem instance use `__iter__`, it's because `Iter` and `GetItem` are comparable
reveal_type(GetItem() in B())  # revealed: bool
```

## Invalid Old-Style Iteration

If `__getitem__` is implemented but does not accept integer arguments, then membership test is not supported and should reveal Never.

```py
class A:
    def __getitem__(self, key: str) -> str: ...


# TODO should be Never
reveal_type(42 in A())  # revealed: bool
reveal_type("hello" in A())  # revealed: bool
```
