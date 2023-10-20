assert True  # S101


def fn():
    x = 1
    assert x == 1  # S101
    assert x == 2  # S101


from typing import TYPE_CHECKING

if TYPE_CHECKING:
    assert True  # OK
