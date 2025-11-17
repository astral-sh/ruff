from decimal import Decimal
from fractions import Fraction
import decimal
import fractions

# Errors
_ = Fraction.from_float(0.1)
_ = Fraction.from_float(-0.5)
_ = Fraction.from_float(5.0)
_ = fractions.Fraction.from_float(4.2)
_ = Fraction.from_decimal(Decimal("4.2"))
_ = Fraction.from_decimal(Decimal("-4.2"))
_ = Fraction.from_decimal(Decimal.from_float(4.2))
_ = Decimal.from_float(0.1)
_ = Decimal.from_float(-0.5)
_ = Decimal.from_float(5.0)
_ = decimal.Decimal.from_float(4.2)
_ = Decimal.from_float(float("inf"))
_ = Decimal.from_float(float("-inf"))
_ = Decimal.from_float(float("Infinity"))
_ = Decimal.from_float(float("-Infinity"))
_ = Decimal.from_float(float("nan"))
_ = Decimal.from_float(float("-NaN "))
_ = Decimal.from_float(float(" \n+nan   \t"))
_ = Decimal.from_float(float("  iNf \n\t "))
_ = Decimal.from_float(float("   -inF\n \t"))
_ = Decimal.from_float(float("  InfinIty \n\t "))
_ = Decimal.from_float(float("   -InfinIty\n \t"))

# Cases with keyword arguments - should produce unsafe fixes
_ = Fraction.from_decimal(dec=Decimal("4.2"))
_ = Decimal.from_float(f=4.2)

# Cases with invalid argument counts - should not get fixes
_ = Fraction.from_decimal(Decimal("4.2"), 1)
_ = Decimal.from_float(4.2, None)

# Cases with wrong keyword arguments - should not get fixes  
_ = Fraction.from_decimal(numerator=Decimal("4.2"))
_ = Decimal.from_float(value=4.2)

# Cases with type validation issues - should produce unsafe fixes
_ = Decimal.from_float("4.2")  # Invalid type for from_float
_ = Fraction.from_decimal(4.2)  # Invalid type for from_decimal
_ = Fraction.from_float("4.2")  # Invalid type for from_float

# OK - should not trigger the rule
_ = Fraction(0.1)
_ = Fraction(-0.5)
_ = Fraction(5.0)
_ = fractions.Fraction(4.2)
_ = Fraction(Decimal("4.2"))
_ = Fraction(Decimal("-4.2"))
_ = Decimal(0.1)
_ = Decimal(-0.5)
_ = Decimal(5.0)
_ = decimal.Decimal(4.2)

# Cases with int and bool - should produce safe fixes
_ = Decimal.from_float(1)
_ = Decimal.from_float(True)

# Cases with non-finite floats - should produce safe fixes
_ = Decimal.from_float(float("-nan"))
_ = Decimal.from_float(float("\x2dnan"))
_ = Decimal.from_float(float("\N{HYPHEN-MINUS}nan"))

# See: https://github.com/astral-sh/ruff/issues/21257
# fixes must be safe
_ = Fraction.from_float(f=4.2)
_ = Fraction.from_decimal(dec=4)