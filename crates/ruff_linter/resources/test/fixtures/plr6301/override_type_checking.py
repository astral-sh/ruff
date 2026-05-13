# Test case for PLR6301 with override decorator imported in TYPE_CHECKING block
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from typing import override
else:
    try:
        from typing_extensions import override
    except ImportError:
        from typing import override


class Parent:
    def method(self):
        pass


class Child(Parent):
    @override
    def method(self):
        pass