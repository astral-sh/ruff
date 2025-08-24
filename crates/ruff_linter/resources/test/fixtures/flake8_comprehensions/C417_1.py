##### https://github.com/astral-sh/ruff/issues/15809

### Errors

def overshadowed_list():
    list = ...
    list(map(lambda x: x, []))


### No errors

dict(map(lambda k: (k,), a))
dict(map(lambda k: (k, v, 0), a))
dict(map(lambda k: [k], a))
dict(map(lambda k: [k, v, 0], a))
dict(map(lambda k: {k, v}, a))
dict(map(lambda k: {k: 0, v: 1}, a))

a = [(1, 2), (3, 4)]
map(lambda x: [*x, 10], *a)
map(lambda x: [*x, 10], *a, *b)
map(lambda x: [*x, 10], a, *b)


map(lambda x: x + 10, (a := []))
list(map(lambda x: x + 10, (a := [])))
set(map(lambda x: x + 10, (a := [])))
dict(map(lambda x: (x, 10), (a := [])))
