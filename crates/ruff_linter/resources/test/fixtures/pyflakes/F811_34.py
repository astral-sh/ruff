from typing import TYPE_CHECKING
from faststream._internal.broker import BrokerUsecase

if TYPE_CHECKING:
    # See: https://github.com/astral-sh/ruff/issues/22554
    # shoud detect -------------------------vvvvvvvvvvvvv
    from faststream._internal.broker import BrokerUsecase
    from faststream.specification.schema import Contact, License

if TYPE_CHECKING:
    # See: https://github.com/astral-sh/ruff/pull/22560#discussion_r2866237036
    # should not detect
	import pyarrow_hotfix

def foo():
	import pyarrow_hotfix

# Should NOT detect: the runtime import is itself conditional, so the
# TYPE_CHECKING import is not redundant (e.g., needed for type checkers
# when the condition is False).
HAS_THING = True

if HAS_THING:
    from foo import Bar

if TYPE_CHECKING:
    from foo import Bar
