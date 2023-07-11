x = range(10)

# RUF015
list(x)[0]
list(x)[:1]
list(x)[:1:1]
list(x)[:1:2]
tuple(x)[0]
tuple(x)[:1]
tuple(x)[:1:1]
tuple(x)[:1:2]
list(i for i in x)[0]
list(i for i in x)[:1]
list(i for i in x)[:1:1]
list(i for i in x)[:1:2]
[i for i in x][0]
[i for i in x][:1]
[i for i in x][:1:1]
[i for i in x][:1:2]

# OK (not indexing (solely) the first element)
list(x)
list(x)[1]
list(x)[-1]
list(x)[1:]
list(x)[:3:2]
list(x)[::2]
list(x)[::]
[i for i in x]
[i for i in x][1]
[i for i in x][-1]
[i for i in x][1:]
[i for i in x][:3:2]
[i for i in x][::2]
[i for i in x][::]

# OK (doesn't mirror the underlying list)
[i + 1 for i in x][0]
[i for i in x if i > 5][0]
[(i, i + 1) for i in x][0]

# OK (multiple generators)
y = range(10)
[i + j for i in x for j in y][0]