y = 1

if (
    # 1
    x # 2
    := # 3
    y # 4
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

del (x := 1)

try:
    pass
except (e := Exception):
    if x := 1:
        pass

(x := 1)

(x := 1) + (y := 2)
