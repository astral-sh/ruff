import decimal

# Tests with fully qualified import
decimal.Decimal(0)

decimal.Decimal(0.0)  # Should error

decimal.Decimal("0.0")

decimal.Decimal(10)

decimal.Decimal(10.0)  # Should error

decimal.Decimal("10.0")

decimal.Decimal(-10)

decimal.Decimal(-10.0)  # Should error

decimal.Decimal("-10.0")

a = 10.0

decimal.Decimal(a)


# Tests with relative import
from decimal import Decimal


val = Decimal(0)

val = Decimal(0.0)  # Should error

val = Decimal("0.0")

val = Decimal(10)

val = Decimal(10.0)  # Should error

val = Decimal("10.0")

val = Decimal(-10)

val = Decimal(-10.0)  # Should error

val = Decimal("-10.0")

a = 10.0

val = Decimal(a)

# See https://github.com/astral-sh/ruff/issues/13258
val = Decimal(~4.0) # Skip

val = Decimal(++4.0) # Suggest `Decimal("4.0")`

val = Decimal(-+--++--4.0) # Suggest `Decimal("-4.0")` 


# Tests with shadowed name
class Decimal():
    value: float | int | str

    def __init__(self, value: float | int | str) -> None:
        self.value = value


val = Decimal(0.0)

val = Decimal("0.0")

val = Decimal(10.0)

val = Decimal("10.0")

val = Decimal(-10.0)

val = Decimal("-10.0")

a = 10.0

val = Decimal(a)


# Retest with fully qualified import

val = decimal.Decimal(0.0)  # Should error

val = decimal.Decimal("0.0")

val = decimal.Decimal(10.0)  # Should error

val = decimal.Decimal("10.0")

val = decimal.Decimal(-10.0)  # Should error

val = decimal.Decimal("-10.0")

a = 10.0

val = decimal.Decimal(a)


class decimal():
    class Decimal():
        value: float | int | str

        def __init__(self, value: float | int | str) -> None:
            self.value = value


val = decimal.Decimal(0.0)

val = decimal.Decimal("0.0")

val = decimal.Decimal(10.0)

val = decimal.Decimal("10.0")

val = decimal.Decimal(-10.0)

val = decimal.Decimal("-10.0")

a = 10.0

val = decimal.Decimal(a)
