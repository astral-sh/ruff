import pytest

@pytest.mark.parametrize(("param",), [[1], [2]])
def test_PT006_and_PT007_do_not_conflict(param):
    ...
