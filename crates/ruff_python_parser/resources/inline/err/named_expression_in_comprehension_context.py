class C:
    [(x := y) for y in range(3)]
[x for x in (y := [1])]
[x for x in [(y := 1) for y in range(1)]]
class D:
    [x for x in [(y := 1) for y in range(1)]]
