def test_xxx(fixture):  # Ok good param name
    pass


def xxx(_param):  # Ok not a test
    pass


def test_xxx(_fixture):  # Error arg
    pass


def test_xxx(*, _fixture):  # Error kwonly
    pass

# https://github.com/astral-sh/ruff/issues/17599

@pytest.mark.parametrize("_foo", [1, 2, 3])
def test_thingy(_foo):  # Ok defined in parametrize
    pass

@pytest.mark.parametrize(
    "_test_input,_expected",
    [("3+5", 8), ("2+4", 6), pytest.param("6*9", 42, marks=pytest.mark.xfail)],
)
def test_eval(_test_input, _expected):  # OK defined in parametrize
    pass


@pytest.mark.parametrize("_foo", [1, 2, 3])
def test_thingy2(_foo, _bar):  # Error _bar is not defined in parametrize
    pass

@pytest.mark.parametrize(["_foo", "_bar"], [1, 2, 3])
def test_thingy3(_foo, _bar):  # OK defined in parametrize
    pass

@pytest.mark.parametrize(("_foo"), [1, 2, 3])
def test_thingy4(_foo, _bar):  # Error _bar is not defined in parametrize
    pass

@pytest.mark.parametrize(["   _foo", "   _bar    "], [1, 2, 3])
def test_thingy5(_foo, _bar):  # OK defined in parametrize
    pass

x = "other"
@pytest.mark.parametrize(x, [1, 2, 3])
def test_thingy5(_foo, _bar):  # known false negative, we don't try to resolve variables
    pass
