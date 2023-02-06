def test_xxx(fixture):  # Ok good param name
    pass


def xxx(_param):  # Ok not a test
    pass


def test_xxx(_fixture):  # Error arg
    pass


def test_xxx(*, _fixture):  # Error kwonly
    pass
