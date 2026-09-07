# UP035
from collections import Mapping

from collections import Mapping as MAP

from collections import Mapping, Sequence

from collections import Counter, Mapping

from collections import (Counter, Mapping)

from collections import (Counter,
                         Mapping)

from collections import Counter, \
                         Mapping

from collections import Counter, Mapping, Sequence

from collections import Mapping as mapping, Counter

if True:
    from collections import Mapping, Counter

if True:
    if True:
        pass
    from collections import Mapping, Counter

if True: from collections import Mapping

import os
from collections import Counter, Mapping
import sys

if True:
    from collections import (
        Mapping,
        Callable,
        Bad,
        Good,
    )

from typing import Callable, Match, Pattern, List, OrderedDict, AbstractSet, ContextManager

if True: from collections import (
    Mapping, Counter)

# Bad imports from PYI027 that are now handled by PYI022 (UP035)
from typing import ContextManager
from typing import OrderedDict
from typing_extensions import OrderedDict
from typing import Callable
from typing import ByteString
from typing import Container
from typing import Hashable
from typing import ItemsView
from typing import Iterable
from typing import Iterator
from typing import KeysView
from typing import Mapping
from typing import MappingView
from typing import MutableMapping
from typing import MutableSequence
from typing import MutableSet
from typing import Sequence
from typing import Sized
from typing import ValuesView
from typing import Awaitable
from typing import AsyncIterator
from typing import AsyncIterable
from typing import Coroutine
from typing import Collection
from typing import AsyncGenerator
from typing import Reversible
from typing import Generator
from typing import Callable
from typing import cast

# OK
from a import b

# UP035 on py312+ only
from typing_extensions import SupportsIndex

# UP035 on py312+ only
from typing_extensions import NamedTuple

# UP035 on py312+ only: `typing_extensions` supports `frozen_default` (backported from 3.12).
from typing_extensions import dataclass_transform

# UP035
from backports.strenum import StrEnum

# UP035
from typing_extensions import override

# UP035
from typing_extensions import Buffer

# UP035
from typing_extensions import get_original_bases

# UP035 on py313+ only
from typing_extensions import TypeVar

# UP035 on py313+ only
from typing_extensions import CapsuleType

# UP035 on py313+ only
from typing_extensions import deprecated

# UP035 on py313+ only
from typing_extensions import get_type_hints

# https://github.com/astral-sh/ruff/issues/15780
from typing_extensions import is_typeddict
# https://github.com/astral-sh/ruff/pull/15800#pullrequestreview-2580704217
from typing_extensions import TypedDict

# UP035 on py37+ only
from typing.io import BinaryIO

# UP035 on py37+ only
from typing.io import IO

# UP035 on py37+ only
from typing.io import TextIO

# UP035 on py37+ only
from typing.re import Match

# UP035 on py37+ only
from typing.re import Pattern


# Runtime-sensitive UP035 applicability

from typing import TYPE_CHECKING

from typing import Sequence as TypingOnlySequence

if TYPE_CHECKING:
    typing_only_value: TypingOnlySequence

from typing import Sequence as RuntimeSequence

runtime_value = RuntimeSequence

from typing import Sequence as RuntimeAnnotationSequence

def runtime_annotation(value: RuntimeAnnotationSequence) -> None:
    pass

from typing import Sequence as ReexportSequence

__all__ = ["ReexportSequence"]

class RuntimeClass:
    from typing import Sequence as ClassSequence

    value = ClassSequence


def typing_only_local():
    from typing import Sequence as LocalTypingOnlySequence

    if TYPE_CHECKING:
        local_typing_only_value: LocalTypingOnlySequence


def typing_only_local_with_locals():
    from typing import Sequence as LocalWithLocalsSequence

    if TYPE_CHECKING:
        local_with_locals_value: LocalWithLocalsSequence

    locals()


from typing import Sequence as ShadowedSequence

shadowed_runtime_value = ShadowedSequence
ShadowedSequence = list


from typing import Callable as ShadowedCallable, Sequence as MultiTypingOnlySequence

shadowed_callable_runtime = ShadowedCallable
ShadowedCallable = object

if TYPE_CHECKING:
    multi_typing_only_value: MultiTypingOnlySequence
