[x:= 2 for x in range(2)]
[x for x in [1] if (y := x) for y in [1]]
[[x for x in [0] if (x := 1)] for x in [0]]
[x for x in (lambda: (y := [1]))()]
