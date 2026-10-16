# `non-augmented-assignment` (`PLR6104`)

```toml
[lint]
preview = true
select = ["PLR6104"]
```

## Unary operators on literals

When the assignment target is the right-hand operand, the rule only rewrites the assignment if the
other operand is a number or a boolean literal, because the operator has to commute for the rewrite
to preserve behavior.

The parser does not fold constants, so `-1` is a unary `-` applied to `1` rather than a literal. Any
stack of `+`, `-`, `~` or `not` over a number or boolean literal still evaluates to a number or a
boolean, so the operand is peeled before the literal check.

```py
to_multiply = -1 + to_multiply  # snapshot: non-augmented-assignment
to_multiply = +1 * to_multiply  # error: [non-augmented-assignment]
to_multiply = --1 + to_multiply  # error: [non-augmented-assignment]
to_multiply = -1.5 + to_multiply  # error: [non-augmented-assignment]
to_multiply = -1j + to_multiply  # error: [non-augmented-assignment]
flags = ~0x1 & flags  # error: [non-augmented-assignment]
flags = -True | flags  # error: [non-augmented-assignment]
```

```snapshot
error[PLR6104]: Use `+=` to perform an augmented assignment directly
 --> src/mdtest_snippet.py:1:1
  |
1 | to_multiply = -1 + to_multiply  # snapshot: non-augmented-assignment
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with augmented assignment
  |
  - to_multiply = -1 + to_multiply  # snapshot: non-augmented-assignment
1 + to_multiply += -1  # snapshot: non-augmented-assignment
2 | to_multiply = +1 * to_multiply  # error: [non-augmented-assignment]
  |
note: This is an unsafe fix and may change runtime behavior
```

Parentheses around the moved operand are preserved:

```py
to_multiply = (not True) + to_multiply  # snapshot: non-augmented-assignment
```

```snapshot
error[PLR6104]: Use `+=` to perform an augmented assignment directly
 --> src/mdtest_snippet.py:8:1
  |
8 | to_multiply = (not True) + to_multiply  # snapshot: non-augmented-assignment
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with augmented assignment
  |
7 | flags = -True | flags  # error: [non-augmented-assignment]
  - to_multiply = (not True) + to_multiply  # snapshot: non-augmented-assignment
8 + to_multiply += (not True)  # snapshot: non-augmented-assignment
  |
note: This is an unsafe fix and may change runtime behavior
```

## Target already on the left

Commutativity is irrelevant when the target is the left-hand operand, so a unary operand needs no
literal check at all. The right-hand side of an augmented assignment accepts any expression, so the
moved operand never needs new parentheses either.

```py
to_multiply = to_multiply**-1  # snapshot: non-augmented-assignment
to_multiply = to_multiply - -1  # error: [non-augmented-assignment]
```

```snapshot
error[PLR6104]: Use `**=` to perform an augmented assignment directly
 --> src/mdtest_snippet.py:1:1
  |
1 | to_multiply = to_multiply**-1  # snapshot: non-augmented-assignment
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with augmented assignment
  |
  - to_multiply = to_multiply**-1  # snapshot: non-augmented-assignment
1 + to_multiply **= -1  # snapshot: non-augmented-assignment
2 | to_multiply = to_multiply - -1  # error: [non-augmented-assignment]
  |
note: This is an unsafe fix and may change runtime behavior
```

## Unary operators on non-literals

The unary operand is not a literal, so its type is unknown and the operator may not commute.

```py
to_multiply = -a_number + to_multiply
to_multiply = -to_multiply + 1
```

`not` evaluates to a boolean whatever it is applied to, so rewriting the case below would in fact be
safe. The check deliberately stays narrow and only looks for number and boolean literals underneath
the unary operators.

```py
to_multiply = (not "") + to_multiply
```

## Non-commutative operators

`-` does not commute, regardless of the operand's type.

```py
to_multiply = -1 - to_multiply
```
