# For tuple expression, the minimum binding power of star expression is bitwise or.
# Test the first and any other element as the there are two separate calls.

(*x in y, z, *x in y)
(*not x, z, *not x)
(*x and y, z, *x and y)
(*x or y, z, *x or y)
(*x if True else y, z, *x if True else y)
(*lambda x: x, z, *lambda x: x)
(*x := 2, z, *x := 2)


# Non-parenthesized
*x in y, z, *x in y
*not x, z, *not x
*x and y, z, *x and y
*x or y, z, *x or y
*x if True else y, z, *x if True else y
*lambda x: x, z, *lambda x: x
*x := 2, z, *x := 2