from typing import TYPE_CHECKING
from faststream._internal.broker import BrokerUsecase

if TYPE_CHECKING:
    # See: https://github.com/astral-sh/ruff/issues/22554
    # shoud detect -------------------------vvvvvvvvvvvv
    from faststream._internal.broker import BrokerUsecase
    from faststream.specification.schema import Contact, License

if TYPE_CHECKING:
    # See:https://github.com/astral-sh/ruff/pull/22560#discussion_r2866237036
	import pyarrow_hotfix

def foo():
	import pyarrow_hotfix