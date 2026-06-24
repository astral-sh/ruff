def test_case_1():
    # comments preserved with a positional argument
    exit(  # comment
        1,  # 2
    )  # 3


def test_case_2():
    # comments preserved with a single keyword argument
    exit(  # comment
        code=1,  # 2
    )  # 3


def test_case_3():
    # no diagnostic for multiple arguments
    exit(2, 3, 4)


def test_case_4():
    # this should now be fixable
    codes = [1]
    exit(*codes)
