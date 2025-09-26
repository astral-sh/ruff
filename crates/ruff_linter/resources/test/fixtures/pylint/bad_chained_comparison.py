def valid() -> bool:
    x = 5
    a = 1 < x <= 10
    b = 25 >= 20 > x > 5
    return a and b


def invalid_mixed_comparison_identity() -> bool:
    b = 10
    return 5 < b is not None


def invalid_mixed_comparison_membership() -> bool:
    c = 7
    lst = [1, 2, 3, True]
    return 1 < c in lst


def invalid_mixed_complex() -> bool:
    d = 10
    lst = [1, 2, 3, None]
    return 5 < d is not None in lst
