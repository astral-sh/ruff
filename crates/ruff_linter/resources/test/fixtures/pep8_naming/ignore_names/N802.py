import unittest

def badAllowed():
    pass

def stillBad():
    pass

class Test(unittest.TestCase):
    def badAllowed(self):
        return super().tearDown()

    def stillBad(self):
        return super().tearDown()
