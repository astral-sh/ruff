import decimal
from decimal import Decimal
from decimal import Decimal as dc

# Positive cases

Decimal("0")
Decimal("-42")
Decimal(float("Infinity"))
Decimal(float("-Infinity"))
Decimal(float("inf"))
Decimal(float("-inf"))
Decimal(float("nan"))
decimal.Decimal("0")
dc("0")

# Negative cases

Decimal(0)
Decimal("Infinity")
decimal.Decimal(0)
dc(0)