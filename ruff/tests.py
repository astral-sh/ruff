import unittest

from ruff import check


class CheckTest(unittest.TestCase):
    def test_something(self):
        results = check(
            """
import os
import sys

print(os.environ['GREETING'])
        """
        )
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].code, "F401")
        self.assertEqual(results[0].message, "`sys` imported but unused")
        self.assertEqual(results[0].location.row, 3)
        self.assertEqual(results[0].location.column, 7)
        self.assertEqual(results[0].end_location.row, 3)
        self.assertEqual(results[0].end_location.column, 10)

    def test_options(self):
        results = check(
            """
import sys

print("This is a really long text")
            """,
            options={"line-length": 10, "ignore": ["F401"]},
        )
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].code, "E501")
        self.assertEqual(results[0].message, "Line too long (35 > 10 characters)")
        self.assertEqual(results[0].location.row, 4)
        self.assertEqual(results[0].location.column, 10)
        self.assertEqual(results[0].end_location.row, 4)
        self.assertEqual(results[0].end_location.column, 35)

    def test_invalid_options(self):
        with self.assertRaisesRegex(
            Exception,
            "unknown field `line-count`, expected one of `allowed-confusables`, .*",
        ):
            check("print(123)", options={"line-count": 10})


if __name__ == "__main__":
    unittest.main()
