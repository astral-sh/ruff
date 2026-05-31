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

# Tuple constructor form (valid sign/digits/exponent)
d8 = Decimal((0, (1, 2, 3), -2))

# Float literals (deferred to RUF032)
d9 = Decimal(1.5)
d10 = Decimal(3.14)

# Keyword argument with string literal
d11 = Decimal(value="99.9")

# Function parameter annotated as str
def process_str(s: str):
    return decimal.Decimal(s)

# Untyped variable (Unknown - not reported)
z = some_function()
d12 = Decimal(z)

# Attribute access (Unknown - not reported)
d13 = Decimal(obj.value)

# Function call result (Unknown - not reported)
d14 = Decimal(str(100))

# ===== INVALID cases (should trigger RUF076) =====

# Integer literals
d20 = Decimal(1)  # RUF076
d21 = Decimal(0)  # RUF076
d22 = Decimal(0xAB)  # RUF076
d23 = decimal.Decimal(42)  # RUF076

# Unary ops on integers
d24 = Decimal(+1)  # RUF076
d25 = Decimal(-1)  # RUF076

# Boolean literals (subclass of int)
d26 = Decimal(True)  # RUF076
d27 = Decimal(False)  # RUF076

# Complex literals
d28 = Decimal(1j)  # RUF076

# Bytes literal
d29 = Decimal(b"123")  # RUF076

# Variables with int annotation
x: int = 42
d30 = Decimal(x)  # RUF076

# Variables with float annotation
y: float = 3.14
d31 = Decimal(y)  # RUF076

# Keyword argument with non-string value
d32 = Decimal(value=1)  # RUF076
d33 = Decimal(value=0xFF)  # RUF076

# Function parameter annotated as int
def process_int(n: int):
    return decimal.Decimal(n)  # RUF076

# Function parameter annotated as float
def process_float(f: float):
    return decimal.Decimal(f)  # RUF076

# ===== Edge cases =====

# Shadowed Decimal class (should NOT trigger)
class Decimal:
    def __init__(self, value):
        self.value = value

d40 = Decimal(1)  # No error: shadowed name

# Re-test with fully qualified after shadow
d41 = decimal.Decimal(1)  # RUF076: still resolves to real decimal.Decimal
