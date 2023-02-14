"""Test that importing a module twice in a try block does not raise a warning."""

try:
    import os
except:
    import os
os.path
