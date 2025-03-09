##### https://github.com/astral-sh/ruff/issues/15809

### Errors

def overshadowed_list():
    list = ...
    list(map(lambda x: x, []))


set(map(lambda x, y: x, nums, nums))

list(map(lambda x: (a := x), foo))

list(map(lambda x: (a for a in \
                    range(x)), foo))

dict(map(lambda k, v: (k, v), keys, values))

dict(
    map(
        lambda k, v: (
            (  # Foo
                k
            ),
            v **2
        ),
        keys, values
    )
)

list(map(lambda x: list[...], foo))


def unfixable():
    zip = []
    map(lambda x, y: x + y + 1, a, b)


dict(
    map(
        lambda x, y: (
            a := 0,
            b := 1
        ),
        foo, bar
    )
)


dict(
    map(
        lambda x, y: (
            list[...],
        # Comment
            (  # Comment
                a for a \
                in b
            )
        ),
        foo, bar
    )
)


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
