# Errors
some_string = "some string"
index, a_number, to_multiply, to_divide, to_cube, timeDiffSeconds, flags = (
    0,
    1,
    2,
    3,
    4,
    5,
    0x3,
)
a_list = [1, 2]
some_set = {"elem"}
mat1, mat2 = None, None
a_float = 0.5

some_string = some_string + "a very long end of string"
index = index - 1
a_list = a_list + ["to concat"]
some_set = some_set | {"to concat"}
to_multiply = to_multiply * 5
to_multiply = 5 * to_multiply
to_multiply = to_multiply * to_multiply
to_divide = to_divide / 5
to_divide = to_divide // 5
to_cube = to_cube**3
to_cube = 3**to_cube
to_cube = to_cube**to_cube
timeDiffSeconds = timeDiffSeconds % 60
flags = flags & 0x1
flags = flags | 0x1
flags = flags ^ 0x1
flags = flags << 1
flags = flags >> 1
mat1 = mat1 @ mat2
a_list[1] = a_list[1] + 1

a_list[0:2] = a_list[0:2] * 3
a_list[:2] = a_list[:2] * 3
a_list[1:] = a_list[1:] * 3
a_list[:] = a_list[:] * 3

index = index * (index + 10)


class T:
    def t(self):
        self.a = self.a + 1


obj = T()
obj.a = obj.a + 1


a = a+-1

# Regression tests for https://github.com/astral-sh/ruff/issues/11672
test = 0x5
test = test + 0xBA

test2 = b""
test2 = test2 + b"\000"

test3 = ""
test3 = test3 + (   a := R""
                         f"oo"   )

test4 = []
test4 = test4 + ( e
                  for e in
                  range(10)
                  )

test5 = test5 + (
    4
    *
    10
)

test6 = test6 + \
        (
            4
            *
            10
        )

test7 = \
        100 \
    + test7

test8 = \
    886 \
    + \
 \
    test8


# Regression tests for https://github.com/astral-sh/ruff/issues/11656
# Chains of an associative operator.
to_multiply = to_multiply * 2 * a_number * 4
some_string = some_string + "2" + some_string + "4"
a_list = a_list + [1] + [2]
some_set = some_set | {"to"} | {"concat"}
flags = flags & 0x1 & 0x2
flags = flags | 0x1 | 0x2
flags = flags ^ 0x1 ^ 0x2
index = index + 1 + 2 + 3 + 4
index = index * (index + 10) * 2
to_multiply = to_multiply * 2 * (a_number + 1)
index = index \
    + 1 \
    + 2

# A parenthesized group of the same operator is an operand of the chain, not a link in it, so it
# stays grouped.
index = index + 1 + (2 + 3) + 4

# Targets that aren't a plain name, including one spread over several lines.
a_list[1] = a_list[1] + 1 + 2
a_list[
    1
] = a_list[
    1
] + 2 + 3

# Parentheses around the target don't reach the AST, so the comparison against the assignment
# target still matches.
index = (index) + 1 + 2

# In a chain, the operands that stay on the right-hand side keep their order, so a side effect in
# one of them still runs exactly once, at the same point in the evaluation.
to_multiply = to_multiply * (a_number := 2) * 3

# Floating-point addition is not associative, so this fix can change the last bits of the result.
# It is reported anyway, in line with the rest of the rule's unsafe fixes.
a_float = a_float + 0.1 + 0.2

# `**` is right-associative, so `to_cube**2**3` is `to_cube ** (2**3)`: not a chain at all, but a
# single operation whose left operand happens to be the target.
to_cube = to_cube**2**3

# The parentheses that keep these continuation lines legal are dropped along with the rest of the
# statement, so the fix has to add its own.
index = (
    index
    + 1
    + 2
)
index = (
    index
    + 1  # a comment inside the chain
    + 2
)
to_multiply = (
    to_multiply
    * 2
    * (a_number + 1)
)
index = (
    1
    + 2
    + index
)
some_string = (
    some_string
    + "implicitly"
      "concatenated"
)

# Chains where the target is the rightmost operand, and every other operand is a number.
to_multiply = 2 * 3 * to_multiply
index = 1 + 2 + index
index = 1 + 2 * 3 + index
to_multiply = 2 * 3 * 4 * to_multiply
flags = 0x1 | 0x2 | flags

# Unary `+`, `-` and `~` applied to a number still give a number.
index = -1 + index
index = -1 + -2 + index
to_multiply = +2 * -3 * to_multiply
flags = ~0x1 | 0x2 | flags


class U:
    def u(self):
        self.a = self.a + 1 + 2


# OK
a_list[0] = a_list[:] * 3
index = a_number = a_number + 1
a_number = index = a_number + 1
index = index * index + 10
some_string = "a very long start to the string" + some_string

# Chains of a non-associative operator: `x = x - 1 - 2` is not `x -= 1 - 2`.
index = index - 1 - 2
to_divide = to_divide / 5 / 2
to_divide = to_divide // 5 // 2
timeDiffSeconds = timeDiffSeconds % 60 % 7
flags = flags << 1 << 2
flags = flags >> 1 >> 2

# Matrix multiplication is associative, but regrouping it can change the cost by orders of
# magnitude, so `@` chains are left alone.
mat1 = mat1 @ mat2 @ mat2

# Mixed operators, where the target is not the leftmost operand of the outermost operation.
index = index * 2 + 3
index = index + 1 - 2

# Parenthesized chains: the parentheses sit in the middle of the text we'd reuse for the fix. The
# second case has them below the outermost operation, where the walk down the left spine has to stop
# just as it does at the top.
to_multiply = (to_multiply * 2) * 4
index = (index + 1) + 2 + 3

# The target is the rightmost operand, but the operator is not commutative.
index = 1 - 2 - index
to_divide = 1 / 2 / to_divide

# The target is the rightmost operand, but the other operands are not all numbers, so the operator
# may not be commutative here. A unary operator doesn't make its operand a number: `-a_number` is
# whatever `a_number.__neg__` returns.
to_multiply = 2 * a_number * to_multiply
some_string = "a" + "b" + some_string
to_multiply = -a_number * 2 * to_multiply
index = -a_number + index
flags = ~a_number | 0x2 | flags

# `not 0` is a number too, but `not` is left out of the numeric-literal check: it only reaches this
# position when parenthesized, since `not 0 + index` parses as `not (0 + index)`.
index = (not 0) + index

# The target is in the middle of the chain; rewriting would require reordering the operands.
to_multiply = 2 * to_multiply * 3
