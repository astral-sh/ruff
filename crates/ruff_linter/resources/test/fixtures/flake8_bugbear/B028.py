import warnings

"""
Should emit:
B028 - on lines 8 and 9
"""

warnings.warn(DeprecationWarning("test"))
warnings.warn(DeprecationWarning("test"), source=None)
warnings.warn(DeprecationWarning("test"), source=None, stacklevel=2)
warnings.warn(DeprecationWarning("test"), stacklevel=1)
