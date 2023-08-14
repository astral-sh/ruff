import unittest


class Test(unittest.TestCase):
    def test_errors(self):
        with self.assertRaises(ValueError):
            raise ValueError

        with self.assertRaises(expected_exception=ValueError):
            raise ValueError

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

        with self.failUnlessRaises(ValueError):
            raise ValueError

    def test_unfixable_error(self):
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
            .assertRaises
            # comment
            (ValueError)
        ):
            raise ValueError
