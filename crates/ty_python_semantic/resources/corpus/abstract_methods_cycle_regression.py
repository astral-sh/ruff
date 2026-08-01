# This caused a cycle in `ClassType::abstract_methods()` in an early version
# of https://github.com/astral-sh/ruff/pull/22898

class name_2:
    try:
        pass
    except* 0 as name_1:
        pass
    assert name_1
name_2()
match name_2():
    case 0:
        import name_1
