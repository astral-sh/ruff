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
sorted(sorted(x, key=foo, reverse=False), reverse=False, key=foo)
sorted(sorted(x, reverse=True), reverse=True)
sorted(reversed(x))
sorted(list(x), key=lambda y: y)
tuple(
    list(
        [x, 3, "hell"\
        "o"]
    )
)
set(set())
set(list())
set(tuple())
sorted(reversed())

# Nested sorts with differing keyword arguments. Not flagged.
sorted(sorted(x, key=lambda y: y))
sorted(sorted(x, key=lambda y: y), key=lambda x: x)
sorted(sorted(x), reverse=True)
sorted(sorted(x, reverse=False), reverse=True)

# Preserve trailing comments.
xxxxxxxxxxx_xxxxx_xxxxx = sorted(
    list(x_xxxx_xxxxxxxxxxx_xxxxx.xxxx()),
    # xxxxxxxxxxx xxxxx xxxx xxx xx Nxxx, xxx xxxxxx3 xxxxxxxxx xx
    # xx xxxx xxxxxxx xxxx xxx xxxxxxxx Nxxx
    key=lambda xxxxx: xxxxx or "",
)

xxxxxxxxxxx_xxxxx_xxxxx = sorted(
    list(x_xxxx_xxxxxxxxxxx_xxxxx.xxxx()),  # xxxxxxxxxxx xxxxx xxxx xxx xx Nxxx
    key=lambda xxxxx: xxxxx or "",
)
