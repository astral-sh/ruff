"""Test that importing a module twice using a nested does not issue a warning."""
try:
    if True:
        if True:
            import os
except:
    import os
os.path
