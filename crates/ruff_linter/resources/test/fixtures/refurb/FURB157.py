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
