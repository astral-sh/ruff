# With parentheses
()
(())
((()), ())
(a,)
(a, b)
(a, b,)
((a, b))

# Without parentheses
a,
a, b
a, b,

# Starred expression
*a,
a, *b
*a | b, *await x, (), *()
(*a,)
(a, *b)
(*a | b, *await x, (), *())

# Named expression
(x := 1,)
(x, y := 2)
(x, y := 2, z)
x, (y := 2), z
