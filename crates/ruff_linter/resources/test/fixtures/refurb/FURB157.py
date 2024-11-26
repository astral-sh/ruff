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

# Ok
Decimal("2e-4")
Decimal("2E-4")
Decimal("_1.234__")
Decimal("2e4")
Decimal("2e+4")
Decimal("2E4")
Decimal("1.2") 
# Ok: even though this is equal to `Decimal(123)`,
# we assume that a developer would
# only write it this way if they meant to.
Decimal("١٢٣") 

# Further subtleties
# https://github.com/astral-sh/ruff/issues/14204
Decimal("-0") # Ok
Decimal("_") # Ok
Decimal(" ") # Ok
Decimal("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000") # Ok

# Non-finite variants
# https://github.com/astral-sh/ruff/issues/14587
Decimal(float(" nan "))          # Decimal(" nan ") 
Decimal(float(" +nan "))         # Decimal(" +nan ")
Decimal(float(" -nan "))         # Decimal(" nan "), notice the difference!
Decimal(float(" inf "))          # Decimal(" inf ")
Decimal(float(" +inf "))         # Decimal(" +inf ")
Decimal(float(" -inf "))         # Decimal(" -inf ")
Decimal(float(" infinity "))     # Decimal(" infinity ")
Decimal(float(" +infinity "))    # Decimal(" +infinity ")
Decimal(float(" -infinity "))    # Decimal(" -infinity ")
