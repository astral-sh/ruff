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


point: Point = Point(3, 4)
measurement: Measurement = Measurement(2.5)
print(point.x + point.y)
print(measurement.value)
print(point.total())
print(measurement.scaled(2.0))
