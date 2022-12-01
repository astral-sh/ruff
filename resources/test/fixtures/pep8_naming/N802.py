import unittest


def Bad():
    pass


def _Bad():
    pass


def BAD():
    pass


def BAD_FUNC():
    pass


def good():
    pass


def _good():
    pass


def good_func():
    pass


def tearDownModule():
    pass


class Test(unittest.TestCase):
    def tearDown(self):
        return super().tearDown()

    def testTest(self):
        assert True
