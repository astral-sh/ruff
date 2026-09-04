# At runtime, listing submodules in __all__ without them being imported is
# valid, and causes them to be included in a star import. See #6523
import sys

__all__ = ["dom", "parsers", "sax", "etree"]  # noqa: F822  # pyright: ignore[reportUnsupportedDunderAll]

if sys.version_info >= (3, 15):
    __all__ += ["is_valid_name"]  # pyright: ignore[reportUnsupportedDunderAll]
    from xml.utils import is_valid_name as is_valid_name, is_valid_text as is_valid_text
