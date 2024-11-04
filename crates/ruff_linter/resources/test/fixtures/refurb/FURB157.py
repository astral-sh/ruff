import decimal
from decimal import Decimal

# Errors
Decimal("0")
Decimal("-42")
Decimal(float("Infinity"))
Decimal(float("-Infinity"))
Decimal(float("inf"))
Decimal(float("-inf"))
Decimal(float("nan"))
decimal.Decimal("0")

# OK
Decimal(0)
Decimal("Infinity")
decimal.Decimal(0)

# Handle Python's Decimal parsing
# See https://github.com/astral-sh/ruff/issues/13807

# Errors
Decimal("1_000")
Decimal("__1____000") 
Decimal("2e4")
Decimal("2e+4")
Decimal("2E4")

# Ok
Decimal("2e-4")
Decimal("2E-4")
Decimal("_1.234__")
# Ok: even though this is equal to `Decimal(123)`,
# we assume that a developer would
# only write it this way if they meant to.
Decimal("١٢٣") 
# Ok: due to floating point errors
# this is not equal to `Decimal(1.2)`.
Decimal("1.2") 
# Ok: This is an error of type `decimal.InvalidOperation`,
# whereas `Decimal(2e4e4)` is a SyntaxError, so
# we leave it as is.
Decimal("2e4e4")