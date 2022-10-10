import unittest


class Suite(unittest.TestCase):
    def test(self) -> None:
        self.assertEquals (1, 2)
        self.assertEquals(1, 2)
        self.assertEqual(3, 4)
