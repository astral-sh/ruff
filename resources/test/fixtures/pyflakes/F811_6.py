"""Test that importing a module twice within an if block does raise a warning."""

i = 2
if i == 1:
    import os
    import os
os.path
