import unittest


class Test(unittest.TestCase):
    def test_errors(self):
        with self.assertRaises(ValueError):
            raise ValueError
        with self.assertRaises(expected_exception=ValueError):
            raise ValueError

        with self.failUnlessRaises(ValueError):
            raise ValueError

        with self.assertRaisesRegex(ValueError, "test"):
            raise ValueError("test")

        with self.assertRaisesRegex(ValueError, expected_regex="test"):
            raise ValueError("test")

        with self.assertRaisesRegex(
            expected_exception=ValueError, expected_regex="test"
        ):
            raise ValueError("test")

        with self.assertRaisesRegex(
            expected_regex="test", expected_exception=ValueError
        ):
            raise ValueError("test")

        with self.assertRaisesRegexp(ValueError, "test"):
            raise ValueError("test")

    def test_unfixable_errors(self):
        with self.assertRaises(ValueError, msg="msg"):
            raise ValueError

        with self.assertRaises(
            # comment
            ValueError
        ):
            raise ValueError

        with (
            self
            # comment
            .assertRaises(ValueError)
        ):
            raise ValueError
