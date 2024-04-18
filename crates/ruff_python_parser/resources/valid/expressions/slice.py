# Various combinations
x[:]
x[1:]
x[:2]
x[1:2]
x[::]
x[1::]
x[:2:]
x[1:2:]
x[::3]
x[1::3]
x[:2:3]
x[1:2:3]

# Named expression
x[y := 2]
x[(y := 2):]
x[y := 2,]

# These are two separate slice elements
x[1,:2,]
