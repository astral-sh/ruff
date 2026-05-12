def weighted(value: int, factor: int = 2) -> int:
    return value * factor


class Offset:
    def __init__(self, base: int, delta: int = 4) -> None:
        self.base = base
        self.delta = delta

    def total(self, scale: int = 2, extra: int = 1) -> int:
        return (self.base + self.delta) * scale + extra


offset = Offset(3)
print(weighted(5))
print(weighted(value=5, factor=3))
print(offset.total())
print(offset.total(extra=5))
