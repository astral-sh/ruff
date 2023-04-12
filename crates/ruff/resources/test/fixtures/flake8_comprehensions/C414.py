x = [1, 2, 3]
list(list(x))
list(tuple(x))
tuple(list(x))
tuple(tuple(x))
set(set(x))
set(list(x))
set(tuple(x))
set(sorted(x))
set(sorted(x, key=lambda y: y))
set(reversed(x))
sorted(list(x))
sorted(tuple(x))
sorted(sorted(x))
sorted(sorted(x, key=lambda y: y))
sorted(reversed(x))
sorted(list(x), key=lambda y: y)
tuple(
    list(
        [x, 3, "hell"\
        "o"]
    )
)
