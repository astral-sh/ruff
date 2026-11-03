from pydantic import validate_call

TYPE_CHECKING = False
if TYPE_CHECKING:
    from .foo import Foo


@validate_call
def baz() -> None:
    x: Foo = ...
