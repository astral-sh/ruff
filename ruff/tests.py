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


if __name__ == "__main__":
    unittest.main()
