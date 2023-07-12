y = 1

if (
    # 1
    x # 2
    # 2.5
    := # 3
    # 3.5
    y # 4
):
    ...

# should not add brackets
if x := y:
    ...

y0 = (y1 := f(x))

f(x:=y, z=True)
