from decimal import Decimal

roundint(3.14)

def roundint(n: N) -> int:
    return int(round(n))

type N = Decimal | float
