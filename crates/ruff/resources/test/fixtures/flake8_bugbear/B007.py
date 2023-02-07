for i in range(10):
    print(i)

print(i)  # name no longer defined on Python 3; no warning yet

for i in range(10):  # name not used within the loop; B007
    print(10)

print(i)  # name no longer defined on Python 3; no warning yet


for _ in range(10):  # _ is okay for a throw-away variable
    print(10)


for i in range(10):
    for j in range(10):
        for k in range(10):  # k not used, i and j used transitively
            print(i + j)


def strange_generator():
    for i in range(10):
        for j in range(10):
            for k in range(10):
                for l in range(10):
                    yield i, (j, (k, l))


for i, (j, (k, l)) in strange_generator():  # i, k not used
    print(j, l)

FMT = "{foo} {bar}"
for foo, bar in [(1, 2)]:
    if foo:
        print(FMT.format(**locals()))

for foo, bar in [(1, 2)]:
    if foo:
        print(FMT.format(**globals()))

for foo, bar in [(1, 2)]:
    if foo:
        print(FMT.format(**vars()))

for foo, bar in [(1, 2)]:
    print(FMT.format(foo=foo, bar=eval("bar")))


def f():
    # Fixable.
    for foo, bar, baz in (["1", "2", "3"],):
        if foo or baz:
            break


def f():
    # Unfixable due to usage of `bar` outside of loop.
    for foo, bar, baz in (["1", "2", "3"],):
        if foo or baz:
            break

    print(bar)


def f():
    # Fixable.
    for foo, bar, baz in (["1", "2", "3"],):
        if foo or baz:
            break

    bar = 1


def f():
    # Fixable.
    for foo, bar, baz in (["1", "2", "3"],):
        if foo or baz:
            break

    bar = 1
    print(bar)


# Unfixable due to trailing underscore (`_line_` wouldn't be considered an ignorable
# variable name).
for line_ in range(self.header_lines):
     fp.readline()
