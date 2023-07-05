x = range(10)

# RUF015
list(x)[0]
list(x)[:1]

# RUF015
[i for i in x][0]
[i for i in x][:1]

y = range(10)
# Fine - multiple generators
[i + j for i in x for j in y][0]

# Fine - not indexing at 0
list(x)[1]
list(x)[1:]
[i for i in x][1]
[i for i in x][1:]
