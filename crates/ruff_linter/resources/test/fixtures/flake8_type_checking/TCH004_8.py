from typing import TYPE_CHECKING
from weakref import WeakKeyDictionary

if TYPE_CHECKING:
    from typing import Any

d = WeakKeyDictionary["Any", "Any"]()
