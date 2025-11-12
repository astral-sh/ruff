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
# In this one case, " -nan ", the fix has to be
# `Decimal(" nan ")`` because `Decimal("-nan") != Decimal(float("-nan"))`
Decimal(float(" -nan "))         # Decimal("nan")
Decimal(float(" inf "))          # Decimal(" inf ")
Decimal(float(" +inf "))         # Decimal(" +inf ")
Decimal(float(" -inf "))         # Decimal(" -inf ")
Decimal(float(" infinity "))     # Decimal(" infinity ")
Decimal(float(" +infinity "))    # Decimal(" +infinity ")
Decimal(float(" -infinity "))    # Decimal(" -infinity ")

# Escape sequence handling in "-nan" case
# Here we do not bother respecting the original whitespace
# and other trivia when offering a fix.
# https://github.com/astral-sh/ruff/issues/16771
Decimal(float("\x2dnan"))
Decimal(float("\x20\x2dnan"))
Decimal(float("\x20\u002dnan"))
Decimal(float("\x20\U0000002dnan"))
Decimal(float("\N{space}\N{hyPHen-MINus}nan"))
Decimal(float("\x20\N{character tabulation}\N{hyphen-minus}nan"))
Decimal(float("   -" "nan"))
Decimal(float("-nAn"))

# Test cases for digit separators (safe fixes)
# https://github.com/astral-sh/ruff/issues/20572
Decimal("15_000_000")  # Safe fix: normalizes separators, becomes Decimal(15_000_000)
Decimal("1_234_567")   # Safe fix: normalizes separators, becomes Decimal(1_234_567)
Decimal("-5_000")      # Safe fix: normalizes separators, becomes Decimal(-5_000)
Decimal("+9_999")      # Safe fix: normalizes separators, becomes Decimal(+9_999)

# Test cases for non-thousands separators
Decimal("12_34_56_78")  # Safe fix: preserves non-thousands separators
Decimal("1234_5678")    # Safe fix: preserves non-thousands separators

# Separators _and_ leading zeros
Decimal("0001_2345")
Decimal("000_1_2345")
Decimal("000_000")

# Test cases for underscores before sign
# https://github.com/astral-sh/ruff/issues/21186
Decimal("_-1")      # Should flag as verbose
Decimal("_+1")      # Should flag as verbose
Decimal("_-1_000")  # Should flag as verbose
