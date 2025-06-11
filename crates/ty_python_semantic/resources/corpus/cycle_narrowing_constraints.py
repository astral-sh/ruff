# Regression test for https://github.com/astral-sh/ruff/issues/17215
# panicked in commit 1a6a10b30
# error message:
# dependency graph cycle querying all_narrowing_constraints_for_expression(Id(8591))

def f(a: A, b: B, c: C):
    unknown_a: UA = make_unknown()
    unknown_b: UB = make_unknown()
    unknown_c: UC = make_unknown()
    unknown_d: UD = make_unknown()

    if unknown_a and unknown_b:
        if unknown_c:
            if unknown_d:
                return a, b, c
