"""Test that importing a module twice in a try block does raise a warning."""

try:
    import os
    import os
except:
    pass
os.path
