y = 1

if (
    # 1
    x # 2
    := # 3
    y # 4
):
    pass

if (
    # 1
    x # 2
    := # 3
    (y) # 4
):
    pass

if (
    # 1
    x # 2
    := # 3
    (y) # 4
    # 5
):
    pass

if (
    # 1
    x # 2
    # 2.5
    := # 3
    # 3.5
    y # 4
):
    pass

if (
    # 1
    x # 2
    # 2.5
    := # 3
    # 3.5
    (  # 4
        y # 5
    )  # 6
):
    pass

if (
    x # 2
    := # 3
    y
):
    pass

y0 = (y1 := f(x))

f(x:=y, z=True)

assert (x := 1)


def f():
    return (x := 1)


for x in (y := [1, 2, 3]):
    pass

async for x in (y := [1, 2, 3]):
    pass

try:
    pass
except (e := Exception):
    if x := 1:
        pass

(x := 1)

(x := 1) + (y := 2)

with (x := 1):
    pass


def f():
    yield (x := 1)


def f():
    yield from (x := 1)


async def f():
    await (x := 1)
