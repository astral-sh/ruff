# Comparison: Tuples

## Heterogeneous

For tuples like `tuple[int, str, Literal[1]]`

### Value Comparisons

"Value Comparisons" refers to the operators: `==`, `!=`, `<`, `<=`, `>`, `>=`

#### Results without Ambiguity

Cases where the result can be definitively inferred as a `BooleanLiteral`.

```py
a = (1, "test", (3, 13), True)
b = (1, "test", (3, 14), False)

reveal_type(a == a)  # revealed: Literal[True]
reveal_type(a != a)  # revealed: Literal[False]
reveal_type(a < a)  # revealed: Literal[False]
reveal_type(a <= a)  # revealed: Literal[True]
reveal_type(a > a)  # revealed: Literal[False]
reveal_type(a >= a)  # revealed: Literal[True]

reveal_type(a == b)  # revealed: Literal[False]
reveal_type(a != b)  # revealed: Literal[True]
reveal_type(a < b)  # revealed: Literal[True]
reveal_type(a <= b)  # revealed: Literal[True]
reveal_type(a > b)  # revealed: Literal[False]
reveal_type(a >= b)  # revealed: Literal[False]
```

Even when tuples have different lengths, comparisons should be handled appropriately.

```py path=different_length.py
a = (1, 2, 3)
b = (1, 2, 3, 4)

reveal_type(a == b)  # revealed: Literal[False]
reveal_type(a != b)  # revealed: Literal[True]
reveal_type(a < b)  # revealed: Literal[True]
reveal_type(a <= b)  # revealed: Literal[True]
reveal_type(a > b)  # revealed: Literal[False]
reveal_type(a >= b)  # revealed: Literal[False]

c = ("a", "b", "c", "d")
d = ("a", "b", "c")

reveal_type(c == d)  # revealed: Literal[False]
reveal_type(c != d)  # revealed: Literal[True]
reveal_type(c < d)  # revealed: Literal[False]
reveal_type(c <= d)  # revealed: Literal[False]
reveal_type(c > d)  # revealed: Literal[True]
reveal_type(c >= d)  # revealed: Literal[True]
```

#### Results with Ambiguity

```py
def bool_instance() -> bool: ...
def int_instance() -> int: ...

a = (bool_instance(),)
b = (int_instance(),)

# TODO: All @Todo should be `bool`
reveal_type(a == a)  # revealed: @Todo
reveal_type(a != a)  # revealed: @Todo
reveal_type(a < a)  # revealed: @Todo
reveal_type(a <= a)  # revealed: @Todo
reveal_type(a > a)  # revealed: @Todo
reveal_type(a >= a)  # revealed: @Todo

reveal_type(a == b)  # revealed: @Todo
reveal_type(a != b)  # revealed: @Todo
reveal_type(a < b)  # revealed: @Todo
reveal_type(a <= b)  # revealed: @Todo
reveal_type(a > b)  # revealed: @Todo
reveal_type(a >= b)  # revealed: @Todo
```

#### Comparison Unsupported

If two tuples contain types that do not support comparison, the result may be `Unknown`.
However, `==` and `!=` are exceptions and can still provide definite results.

```py
a = (1, 2)
b = (1, "hello")

# TODO: should be Literal[False]
reveal_type(a == b)  # revealed: @Todo

# TODO: should be Literal[True]
reveal_type(a != b)  # revealed: @Todo

# TODO: should be Unknown and add more informative diagnostics
reveal_type(a < b)  # revealed: @Todo
reveal_type(a <= b)  # revealed: @Todo
reveal_type(a > b)  # revealed: @Todo
reveal_type(a >= b)  # revealed: @Todo
```

However, if the lexicographic comparison completes without reaching a point where str and int are compared,
Python will still produce a result based on the prior elements.

```py path=short_circuit.py
a = (1, 2)
b = (999999, "hello")

reveal_type(a == b)  # revealed: Literal[False]
reveal_type(a != b)  # revealed: Literal[True]
reveal_type(a < b)  # revealed: Literal[True]
reveal_type(a <= b)  # revealed: Literal[True]
reveal_type(a > b)  # revealed: Literal[False]
reveal_type(a >= b)  # revealed: Literal[False]
```

#### Matryoshka Tuples

```py
a = (1, True, "Hello")
b = (a, a, a)
c = (b, b, b)

reveal_type(c == c)  # revealed: Literal[True]
reveal_type(c != c)  # revealed: Literal[False]
reveal_type(c < c)  # revealed: Literal[False]
reveal_type(c <= c)  # revealed: Literal[True]
reveal_type(c > c)  # revealed: Literal[False]
reveal_type(c >= c)  # revealed: Literal[True]
```

#### Non Boolean Rich Comparisons

```py
class A:
    def __eq__(self, o) -> str: ...
    def __ne__(self, o) -> int: ...
    def __lt__(self, o) -> float: ...
    def __le__(self, o) -> object: ...
    def __gt__(self, o) -> tuple: ...
    def __ge__(self, o) -> list: ...

a = (A(), A())

# TODO: All @Todo should be bool
reveal_type(a == a)  # revealed: @Todo
reveal_type(a != a)  # revealed: @Todo
reveal_type(a < a)  # revealed: @Todo
reveal_type(a <= a)  # revealed: @Todo
reveal_type(a > a)  # revealed: @Todo
reveal_type(a >= a)  # revealed: @Todo
```

### Membership Test Comparisons

"Membership Test Comparisons" refers to the operators `in` and `not in`.

```py
def int_instance() -> int: ...

a = (1, 2)
b = ((3, 4), (1, 2))
c = ((1, 2, 3), (4, 5, 6))
d = ((int_instance(), int_instance()), (int_instance(), int_instance()))

reveal_type(a in b)  # revealed: Literal[True]
reveal_type(a not in b)  # revealed: Literal[False]

reveal_type(a in c)  # revealed: Literal[False]
reveal_type(a not in c)  # revealed: Literal[True]

# TODO: All @Todo should be bool
reveal_type(a in d)  # revealed: @Todo
reveal_type(a not in d)  # revealed: @Todo
```

### Identity Comparisons

"Identity Comparisons" refers to `is` and `is not`.

```py
a = (1, 2)
b = ("a", "b")
c = (1, 2, 3)

reveal_type(a is (1, 2))  # revealed: bool
reveal_type(a is not (1, 2))  # revealed: bool

# TODO: Update to Literal[False] once str == int comparison is implemented
reveal_type(a is b)  # revealed: @Todo
# TODO: Update to Literal[True] once str == int comparison is implemented
reveal_type(a is not b)  # revealed: @Todo

reveal_type(a is c)  # revealed: Literal[False]
reveal_type(a is not c)  # revealed: Literal[True]
```

## Homogeneous

For tuples like `tuple[int, ...]`, `tuple[Any, ...]`

// TODO
