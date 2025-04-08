x = (1, 2, 3)

(x, y) = (1, 2, 3)

[x, y] = (1, 2, 3)

x.y = (1, 2, 3)

x[y] = (1, 2, 3)

(x, *y) = (1, 2, 3)


# This last group of tests checks that assignments we expect to be parsed
# (including some interesting ones) continue to be parsed successfully.

[x, y, z] = [1, 2, 3]

(x, y, z) = (1, 2, 3)
x[0] = 42

# This is actually a type error, not a syntax error. So check that it
# doesn't fail parsing.

5[0] = 42
x[1:2] = [42]

# This is actually a type error, not a syntax error. So check that it
# doesn't fail parsing.
5[1:2] = [42]

foo.bar = 42

# This is actually an attribute error, not a syntax error. So check that
# it doesn't fail parsing.
"foo".y = 42

foo = 42

[] = *data
() = *data
a, b = ab
a = b = c