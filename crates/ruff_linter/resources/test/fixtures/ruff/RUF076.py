import decimal
from decimal import Decimal

# ===== VALID cases (should NOT trigger) =====

# String literals
d1 = Decimal("1.23")
d2 = Decimal("0.1")
d3 = Decimal("0")
d4 = decimal.Decimal("10.5")

# String variables with annotation
s: str = "3.14"
d5 = Decimal(s)

# F-strings (resolve to str)
x_val = 42
d6 = Decimal(f"{x_val}")

# No arguments (valid default)
d7 = Decimal()

# ===== INVALID cases (should trigger RUF076) =====

# Integer literals
d8 = Decimal(1)  # RUF076
d9 = Decimal(0)  # RUF076
d10 = Decimal(0xAB)  # RUF076
d11 = decimal.Decimal(42)  # RUF076

# Unary ops on integers
d12 = Decimal(+1)  # RUF076
d13 = Decimal(-1)  # RUF076

# Variables with int annotation
x: int = 42
d14 = Decimal(x)  # RUF076

# Variables with float annotation
y: float = 3.14
d15 = Decimal(y)  # RUF076

# Untyped variable (assigned int literal - type is Unknown, no str annotation)
z = 100
d16 = Decimal(z)  # RUF076

# ===== Edge cases =====

# Shadowed Decimal class (should NOT trigger)
class Decimal:
    def __init__(self, value):
        self.value = value

d17 = Decimal(1)  # No error: shadowed name

# Re-test with fully qualified after shadow
d18 = decimal.Decimal(1)  # RUF076: still resolves to real decimal.Decimal
