pairs = [(1, 2)]


class C:
    [(x := y) for (x, y) in pairs]
    [(lambda x=(x := y): x) for (x, y) in pairs]

[x for x in (lambda: (y := [1]))()]
[x for x in range(1) for y in (lambda: (z := [1]))()]

[(lambda: (x := y))() for (x, y) in pairs]
[(lambda x=(x := y): x) for (x, y) in pairs]
