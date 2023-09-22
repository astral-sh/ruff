"""
Should emit:
B017 - on lines 23 and 41
"""
import asyncio
import unittest
import pytest

CONSTANT = True


def something_else() -> None:
    for i in (1, 2, 3):
        print(i)


class Foo:
    pass


class Foobar(unittest.TestCase):
    def evil_raises(self) -> None:
        with self.assertRaises(Exception):
            raise Exception("Evil I say!")

    def also_evil_raises(self) -> None:
        with self.assertRaises(BaseException):
            raise Exception("Evil I say!")

    def context_manager_raises(self) -> None:
        with self.assertRaises(Exception) as ex:
            raise Exception("Context manager is good")
        self.assertEqual("Context manager is good", str(ex.exception))

    def regex_raises(self) -> None:
        with self.assertRaisesRegex(Exception, "Regex is good"):
            raise Exception("Regex is good")

    def raises_with_absolute_reference(self):
        with self.assertRaises(asyncio.CancelledError):
            Foo()


def test_pytest_raises():
    with pytest.raises(Exception):
        raise ValueError("Hello")

    with pytest.raises(Exception), pytest.raises(ValueError):
        raise ValueError("Hello")

    with pytest.raises(Exception, "hello"):
        raise ValueError("This is fine")

    with pytest.raises(Exception, match="hello"):
        raise ValueError("This is also fine")
