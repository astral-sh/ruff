x[x := 1:]

# Starred expression
x[*x:]
x[:*x]
x[::*x]

# Empty slice
x[]

# Mixed starred expression and named expression
x[*x := 1]