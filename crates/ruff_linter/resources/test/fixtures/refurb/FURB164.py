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

# OK
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
