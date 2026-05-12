class Point:
    def __init__(self, x: int, y: int) -> None:
        self.x = x
        self.y = y

    def total(self) -> int:
        return self.x + self.y


class Measurement:
    def __init__(self, value: float) -> None:
        self.value = value

    def scaled(self, factor: float) -> float:
        return self.value * factor


point = Point(3, 4)
measurement = Measurement(2.5)

point.x = 8
measurement.value = 3.5

print(point.total())
print(measurement.scaled(2.0))
