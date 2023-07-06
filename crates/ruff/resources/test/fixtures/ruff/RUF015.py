x = range(10)

# RUF015
list(x)[0]
list(x)[:1]
list(x)[:1:1]
list(x)[:1:2]
[i for i in x][0]
[i for i in x][:1]
[i for i in x][:1:1]
[i for i in x][:1:2]

# Fine - not indexing (solely) the first element
list(x)
list(x)[1]
list(x)[-1]
list(x)[1:]
list(x)[:3:2]
list(x)[::2]
[i for i in x]
[i for i in x][1]
[i for i in x][-1]
[i for i in x][1:]
[i for i in x][:3:2]
[i for i in x][::2]

# Fine - modifies the first element
[i + 1 for i in x][0]

y = range(10)
# Fine - multiple generators
[i + j for i in x for j in y][0]

