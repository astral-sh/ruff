import unittest
import pytest


class Test(unittest.TestCase):
    def test_pytest_raises(self):
        with pytest.raises(ValueError):
            raise ValueError

    def test_errors(self):
        with self.assertRaises(ValueError):
            raise ValueError

    def test_rewrite_references(self):
        with self.assertRaises(ValueError) as e:
            raise ValueError

        print(e.foo)
        print(e.exception)

    def test_rewrite_references_multiple_items(self):
        with self.assertRaises(ValueError) as e1, \
            self.assertRaises(ValueError) as e2:
            raise ValueError

        print(e1.foo)
        print(e1.exception)

        print(e2.foo)
        print(e2.exception)

    def test_rewrite_references_multiple_items_nested(self):
        with self.assertRaises(ValueError) as e1, \
            foo(self.assertRaises(ValueError)) as e2:
            raise ValueError

        print(e1.foo)
        print(e1.exception)

        print(e2.foo)
        print(e2.exception)
