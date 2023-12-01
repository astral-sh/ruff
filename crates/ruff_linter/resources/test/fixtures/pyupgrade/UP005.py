import unittest


class Suite(unittest.TestCase):
    def test(self) -> None:
        self.assertEquals (1, 2)
        self.assertEquals(1, 2)
        self.assertEqual(3, 4)
        self.failUnlessAlmostEqual(1, 1.1)
        self.assertNotRegexpMatches("a", "b")
