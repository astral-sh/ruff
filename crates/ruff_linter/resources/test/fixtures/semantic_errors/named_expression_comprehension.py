pairs = [(1, 2)]


class C:
    [(x := y) for (x, y) in pairs]


[(lambda: (x := y))() for (x, y) in pairs]
