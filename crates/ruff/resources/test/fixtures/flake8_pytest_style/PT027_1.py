import unittest
import pytest


class Test(unittest.TestCase):
    def test_pytest_raises(self):
        with pytest.raises(ValueError):
            raise ValueError

    def test_errors(self):
        with self.assertRaises(ValueError):
            raise ValueError
