# `non-augmented-assignment` (`PLR6104`)

```toml
[lint]
preview = true
select = ["PLR6104"]
```

## Chains of the same operator

Python parses `x * 2 * y` as `(x * 2) * y`, so in a chain of a single operator the target sits at
the bottom of the left spine. Everything to the right of it is sliced out of the source and reused
as the right-hand side of the augmented assignment.

```py
to_multiply = to_multiply * 2 * a_number * 4  # snapshot: non-augmented-assignment
index = index + 1 + 2 + 3 + 4  # error: [non-augmented-assignment]
```

```snapshot
error[PLR6104]: Use `*=` to perform an augmented assignment directly
 --> src/mdtest_snippet.py:1:1
  |
1 | to_multiply = to_multiply * 2 * a_number * 4  # snapshot: non-augmented-assignment
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with augmented assignment
  |
  - to_multiply = to_multiply * 2 * a_number * 4  # snapshot: non-augmented-assignment
1 + to_multiply *= 2 * a_number * 4  # snapshot: non-augmented-assignment
2 | index = index + 1 + 2 + 3 + 4  # error: [non-augmented-assignment]
  |
note: This is an unsafe fix and may change runtime behavior
```

Regrouping the operands is only sound for an operator that is both commutative and associative,
which for this rule means `+`, `*`, `&`, `|` and `^`.

```py
some_string = some_string + "2" + some_string + "4"  # error: [non-augmented-assignment]
a_list = a_list + [1] + [2]  # error: [non-augmented-assignment]
some_set = some_set | {"to"} | {"concat"}  # error: [non-augmented-assignment]
flags = flags & 0x1 & 0x2  # error: [non-augmented-assignment]
flags = flags | 0x1 | 0x2  # error: [non-augmented-assignment]
flags = flags ^ 0x1 ^ 0x2  # error: [non-augmented-assignment]
```

The operands that stay on the right-hand side are arbitrary expressions, and may mention the target
again:

```py
index = index * (index + 10) * 2  # error: [non-augmented-assignment]
to_multiply = to_multiply * 2 * (a_number + 1)  # error: [non-augmented-assignment]
```

## Parentheses in a chain

A parenthesized group of the same operator is an operand of the chain rather than a link in it, so
it is sliced out along with the rest and stays grouped.

```py
index = index + 1 + (2 + 3) + 4  # error: [non-augmented-assignment]
```

Parentheses around the target itself don't reach the AST, so the comparison against the assignment
target still matches.

```py
index = (index) + 1 + 2  # error: [non-augmented-assignment]
```

Parentheses around a *link* of the chain are a different matter: they sit in the middle of the
source text the fix would reuse verbatim, so the walk down the left spine stops there and the target
is never reached. In the second case the parentheses are below the outermost operation, where the
walk has to stop just as it does at the top.

```py
to_multiply = (to_multiply * 2) * 4
index = (index + 1) + 2 + 3
```

## Targets other than a plain name

The target's own source is sliced out too, so any assignment target works, including one spread over
several lines.

```py
a_list[1] = a_list[1] + 1 + 2  # error: [non-augmented-assignment]
some_obj.attr = some_obj.attr + 1 + 2  # error: [non-augmented-assignment]
```

```py
# snapshot: non-augmented-assignment
a_list[
    1
] = a_list[
    1
] + 2 + 3
```

```snapshot
error[PLR6104]: Use `+=` to perform an augmented assignment directly
 --> src/mdtest_snippet.py:4:1
  |
4 | / a_list[
5 | |     1
6 | | ] = a_list[
7 | |     1
8 | | ] + 2 + 3
  | |_________^
help: Replace with augmented assignment
  |
5 |     1
  - ] = a_list[
  -     1
  - ] + 2 + 3
6 + ] += 2 + 3
  |
note: This is an unsafe fix and may change runtime behavior
```

## Multi-line assignments

A multi-line right-hand side may only have been valid because the assigned value was parenthesized.
Those parentheses are dropped along with the rest of the statement, so the fix adds a fresh pair
around the operand.

```py
# snapshot: non-augmented-assignment
index = (
    index
    + 1
    + 2
)
```

```snapshot
error[PLR6104]: Use `+=` to perform an augmented assignment directly
 --> src/mdtest_snippet.py:2:1
  |
2 | / index = (
3 | |     index
4 | |     + 1
5 | |     + 2
6 | | )
  | |_^
help: Replace with augmented assignment
  |
1 | # snapshot: non-augmented-assignment
  - index = (
  -     index
  -     + 1
  -     + 2
  - )
2 + index += (1
3 +     + 2)
4 | # snapshot: non-augmented-assignment
  |
note: This is an unsafe fix and may change runtime behavior
```

The same applies to a chain held together by backslash continuations.

```py
# snapshot: non-augmented-assignment
index = index \
    + 1 \
    + 2
```

```snapshot
error[PLR6104]: Use `+=` to perform an augmented assignment directly
  --> src/mdtest_snippet.py:8:1
   |
 8 | / index = index \
 9 | |     + 1 \
10 | |     + 2
   | |_______^
help: Replace with augmented assignment
   |
7  | # snapshot: non-augmented-assignment
   - index = index \
   -     + 1 \
   -     + 2
8  + index += (1 \
9  +     + 2)
10 | # snapshot: non-augmented-assignment
   |
note: This is an unsafe fix and may change runtime behavior
```

The fix replaces the whole statement, so a comment inside the chain is lost.

```py
# snapshot: non-augmented-assignment
index = (
    index
    + 1  # a comment inside the chain
    + 2
)
```

```snapshot
error[PLR6104]: Use `+=` to perform an augmented assignment directly
  --> src/mdtest_snippet.py:12:1
   |
12 | / index = (
13 | |     index
14 | |     + 1  # a comment inside the chain
15 | |     + 2
16 | | )
   | |_^
help: Replace with augmented assignment
   |
11 | # snapshot: non-augmented-assignment
   - index = (
   -     index
   -     + 1  # a comment inside the chain
   -     + 2
   - )
12 + index += (1  # a comment inside the chain
13 +     + 2)
14 | # snapshot: non-augmented-assignment
   |
note: This is an unsafe fix and may change runtime behavior
```

An implicitly concatenated string spanning several lines is a single operand, and needs the added
parentheses just the same.

```py
# snapshot: non-augmented-assignment
some_string = (
    some_string
    + "implicitly"
      "concatenated"
)
```

```snapshot
error[PLR6104]: Use `+=` to perform an augmented assignment directly
  --> src/mdtest_snippet.py:18:1
   |
18 | / some_string = (
19 | |     some_string
20 | |     + "implicitly"
21 | |       "concatenated"
22 | | )
   | |_^
help: Replace with augmented assignment
   |
17 | # snapshot: non-augmented-assignment
   - some_string = (
   -     some_string
   -     + "implicitly"
   -       "concatenated"
   - )
18 + some_string += ("implicitly"
19 +       "concatenated")
20 | # error: [non-augmented-assignment]
   |
note: This is an unsafe fix and may change runtime behavior
```

The added pair wraps the whole span that is sliced out, so operands that carry their own parentheses
end up nested inside it.

```py
# error: [non-augmented-assignment]
to_multiply = (
    to_multiply
    * 2
    * (a_number + 1)
)
```

## Regrouping a chain keeps the operands in order

The operands that stay on the right-hand side are neither reordered nor re-evaluated, so a side
effect in one of them still runs exactly once, at the same point in the evaluation.

```py
to_multiply = to_multiply * (a_number := 2) * 3  # error: [non-augmented-assignment]
```

## Floating-point addition is not associative

Regrouping `(a + 0.1) + 0.2` into `a + (0.1 + 0.2)` can change the last bits of the result. It is
reported anyway, in line with the rest of the rule's unsafe fixes.

```py
a_float = a_float + 0.1 + 0.2  # error: [non-augmented-assignment]
```

## Right-associative `**`

`**` is right-associative, so `to_cube**2**3` is `to_cube ** (2**3)`: not a chain at all, but a
single operation whose left operand happens to be the target.

```py
to_cube = to_cube**2**3  # error: [non-augmented-assignment]
```

## Chains of an operator that can't be rearranged

An operator that isn't associative can't be regrouped: `x = x - 1 - 2` is not `x -= 1 - 2`.

```py
index = index - 1 - 2
to_divide = to_divide / 5 / 2
to_divide = to_divide // 5 // 2
seconds = seconds % 60 % 7
flags = flags << 1 << 2
flags = flags >> 1 >> 2
```

Matrix multiplication is associative, but the grouping determines how much arithmetic is done: for a
vector `mat1`, `(mat1 @ mat2) @ mat3` is a pair of cheap matrix-vector products, while
`mat1 @= mat2 @ mat3` builds the full matrix-matrix product first. `@` chains are left alone.

```py
mat1 = mat1 @ mat2 @ mat3
```

## Mixed operators in one expression

The walk down the left spine only follows links that repeat the outermost operator, so `index * 2`
is where it stops and the target is out of reach. The second case doesn't get that far: the
outermost operator is `-`, which never allows its operands to be rearranged.

```py
index = index * 2 + 3
index = index + 1 - 2
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

## Chains where the target is the rightmost operand

When the target is the right-hand operand of the outermost operation, the left-hand side is reused
verbatim, keeping its own grouping. That covers chains as well as single operations, as long as
every other operand is a number.

```py
to_multiply = 2 * 3 * to_multiply  # snapshot: non-augmented-assignment
index = 1 + 2 + index  # error: [non-augmented-assignment]
index = 1 + 2 * 3 + index  # error: [non-augmented-assignment]
to_multiply = 2 * 3 * 4 * to_multiply  # error: [non-augmented-assignment]
flags = 0x1 | 0x2 | flags  # error: [non-augmented-assignment]
```

```snapshot
error[PLR6104]: Use `*=` to perform an augmented assignment directly
 --> src/mdtest_snippet.py:1:1
  |
1 | to_multiply = 2 * 3 * to_multiply  # snapshot: non-augmented-assignment
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: Replace with augmented assignment
  |
  - to_multiply = 2 * 3 * to_multiply  # snapshot: non-augmented-assignment
1 + to_multiply *= 2 * 3  # snapshot: non-augmented-assignment
2 | index = 1 + 2 + index  # error: [non-augmented-assignment]
  |
note: This is an unsafe fix and may change runtime behavior
```

Unary `+`, `-` and `~` applied to a number still give a number.

```py
index = -1 + -2 + index  # error: [non-augmented-assignment]
to_multiply = +2 * -3 * to_multiply  # error: [non-augmented-assignment]
flags = ~0x1 | 0x2 | flags  # error: [non-augmented-assignment]
```

A multi-line left-hand side gets the same added parentheses as a multi-line right-hand side.

```py
# snapshot: non-augmented-assignment
index = (
    1
    + 2
    + index
)
```

```snapshot
error[PLR6104]: Use `+=` to perform an augmented assignment directly
  --> src/mdtest_snippet.py:10:1
   |
10 | / index = (
11 | |     1
12 | |     + 2
13 | |     + index
14 | | )
   | |_^
help: Replace with augmented assignment
   |
9  | # snapshot: non-augmented-assignment
   - index = (
   -     1
   -     + 2
   -     + index
   - )
10 + index += (1
11 +     + 2)
12 | index = 1 - 2 - index
   |
note: This is an unsafe fix and may change runtime behavior
```

The operator still has to commute, so a chain of `-` or `/` is left alone even though every operand
is a number.

```py
index = 1 - 2 - index
to_divide = 1 / 2 / to_divide
```

And the other operands still all have to be numbers, since an arbitrary type may overload the
operator to mean something that doesn't commute. A unary operator doesn't make its operand a number:
`-a_number` is whatever `a_number.__neg__` returns.

```py
to_multiply = 2 * a_number * to_multiply
some_string = "a" + "b" + some_string
to_multiply = -a_number * 2 * to_multiply
flags = ~a_number | 0x2 | flags
```

## Target in the middle of a chain

Rewriting this would require reordering the operands, not just regrouping them.

```py
to_multiply = 2 * to_multiply * 3
```
