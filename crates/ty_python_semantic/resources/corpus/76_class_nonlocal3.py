# Based on Python-3.4.3/Lib/test/test_scope.py

def testNonLocalClass(self):

    def f(x):
        class c:
            nonlocal x
            x += 1
            def get(self):
                return x
        return c()
