[x:= 2 for x in range(2)]
[x for x in (lambda: (y := [1]))()]

class C:
    [(x := y) for y in range(3)]
