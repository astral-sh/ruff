def twice(value: int):
    return value * 2


class Counter:
    def __init__(self, value: int) -> None:
        self.value = value

    def bumped(self, amount: int):
        return self.value + amount


counter = Counter(5)
print(twice(4))
print(counter.bumped(3))
