"""Test that importing a module twice in if-else blocks does not raise a warning."""

i = 2
if i == 1:
    import os
else:
    import os
os.path
