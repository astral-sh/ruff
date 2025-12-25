# Regression test for https://github.com/astral-sh/ruff/issues/17215
# panicked in commit 1a6a10b30
# error message:
# dependency graph cycle querying all_negative_narrowing_constraints_for_expression(Id(859f))

def f(f1: bool, f2: bool, f3: bool, f4: bool):
    o1: UnknownClass = make_o()
    o2: UnknownClass = make_o()
    o3: UnknownClass = make_o()
    o4: UnknownClass = make_o()

    if f1 and f2 and f3 and f4:
        if o1 == o2:
            return None
        if o2 == o3:
            return None
        if o3 == o4:
            return None
        if o4 == o1:
            return None

    return o1, o2, o3, o4
