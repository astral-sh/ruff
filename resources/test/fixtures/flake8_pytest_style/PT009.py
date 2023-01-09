import unittest


class Test(unittest.TestCase):
    def test_xxx(self):
        assert 1 == 1  # OK no parameters

    def test_assert_true(self):
        expr = 1
        msg = "Must be True"
        self.assertTrue(expr)  # Error
        self.assertTrue(expr=expr)  # Error
        self.assertTrue(expr, msg)  # Error
        self.assertTrue(expr=expr, msg=msg)  # Error
        self.assertTrue(msg=msg, expr=expr)  # Error
        self.assertTrue(*(expr, msg))  # Error, unfixable
        self.assertTrue(**{"expr": expr, "msg": msg})  # Error, unfixable
        self.assertTrue(msg=msg, expr=expr, unexpected_arg=False)  # Error, unfixable
        self.assertTrue(msg=msg)  # Error, unfixable

    def test_assert_false(self):
        self.assertFalse(True)  # Error

    def test_assert_equal(self):
        self.assertEqual(1, 2)  # Error

    def test_assert_not_equal(self):
        self.assertNotEqual(1, 1)  # Error

    def test_assert_greater(self):
        self.assertGreater(1, 2)  # Error

    def test_assert_greater_equal(self):
        self.assertGreaterEqual(1, 2)  # Error

    def test_assert_less(self):
        self.assertLess(2, 1)  # Error

    def test_assert_less_equal(self):
        self.assertLessEqual(1, 2)  # Error

    def test_assert_in(self):
        self.assertIn(1, [2, 3])  # Error

    def test_assert_not_in(self):
        self.assertNotIn(2, [2, 3])  # Error

    def test_assert_is_none(self):
        self.assertIsNone(0)  # Error

    def test_assert_is_not_none(self):
        self.assertIsNotNone(0)  # Error

    def test_assert_is(self):
        self.assertIs([], [])  # Error

    def test_assert_is_not(self):
        self.assertIsNot(1, 1)  # Error

    def test_assert_is_instance(self):
        self.assertIsInstance(1, str)  # Error

    def test_assert_is_not_instance(self):
        self.assertNotIsInstance(1, int)  # Error

    def test_assert_regex(self):
        self.assertRegex("abc", r"def")  # Error

    def test_assert_not_regex(self):
        self.assertNotRegex("abc", r"abc")  # Error

    def test_assert_regexp_matches(self):
        self.assertRegexpMatches("abc", r"def")  # Error

    def test_assert_not_regexp_matches(self):
        self.assertNotRegex("abc", r"abc")  # Error
