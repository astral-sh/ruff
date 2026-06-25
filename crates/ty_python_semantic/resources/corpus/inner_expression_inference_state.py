# This is a regression test for `store_expression_type`.
# ref: https://github.com/astral-sh/ty/issues/1688

x: int

type x[T] = x[T, U]
