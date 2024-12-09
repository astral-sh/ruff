import warnings

"""
Should emit:
B028 - on lines 8 and 9
"""

warnings.warn("test", DeprecationWarning)
warnings.warn("test", DeprecationWarning, source=None)
warnings.warn("test", DeprecationWarning, source=None, stacklevel=2)
warnings.warn("test", DeprecationWarning, stacklevel=1)
warnings.warn("test", DeprecationWarning, 1)
warnings.warn("test", category=DeprecationWarning, stacklevel=1)
