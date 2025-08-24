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
args = ("test", DeprecationWarning, 1)
warnings.warn(*args)
kwargs = {"message": "test", "category": DeprecationWarning, "stacklevel": 1}
warnings.warn(**kwargs)
args = ("test", DeprecationWarning)
kwargs = {"stacklevel": 1}
warnings.warn(*args, **kwargs)

warnings.warn(
        "test",
        DeprecationWarning,
        # some comments here
        source = None # no trailing comma
    )

# https://github.com/astral-sh/ruff/issues/18011
warnings.warn("test", skip_file_prefixes=(os.path.dirname(__file__),))
# trigger diagnostic if `skip_file_prefixes` is present and set to the default value
warnings.warn("test", skip_file_prefixes=())

_my_prefixes = ("this","that")
warnings.warn("test", skip_file_prefixes = _my_prefixes)
