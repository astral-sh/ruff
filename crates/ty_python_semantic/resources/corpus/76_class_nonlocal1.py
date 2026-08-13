# Based on Python-3.4.3/Lib/test/test_scope.py

def testClassNamespaceOverridesClosure(self):
    x = 42
    class X:
        locals()["x"] = 43
        y = x
