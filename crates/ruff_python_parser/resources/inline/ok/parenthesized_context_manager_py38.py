# parse_options: {"target-version": "3.8"}
with (x, y) as foo:
    pass

with (x,
    y) as foo:
    pass

with (x,
y):
    pass

with (
    x,
):
    pass

with (
    x,
    y
) as foo:
    pass

with x, (
    y
): pass

with x as foo, (
y
) as bar:
    pass

with x() as foo, (
    y()
) as bar:
    pass
