"""
Should emit:
B017 - on lines 20, 21, 25, and 26
"""
import unittest
import pytest


def something_else() -> None:
    for i in (1, 2, 3):
        print(i)


class Foo:
    pass


class Foobar(unittest.TestCase):
    def call_form_raises(self) -> None:
        self.assertRaises(Exception, something_else)
        self.assertRaises(BaseException, something_else)


def test_pytest_call_form() -> None:
    pytest.raises(Exception, something_else)
    pytest.raises(BaseException, something_else)

    pytest.raises(Exception, something_else, match="hello")
