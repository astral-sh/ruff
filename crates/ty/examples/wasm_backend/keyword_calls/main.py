def combine(left: int, right: int) -> int:
    return left * 10 + right


class Offset:
    def __init__(self, base: int, delta: int) -> None:
        self.base = base
        self.delta = delta

    def total(self, scale: int, extra: int) -> int:
        return (self.base + self.delta) * scale + extra


offset = Offset(delta=4, base=3)
print(combine(right=2, left=1))
print(offset.total(extra=5, scale=2))
