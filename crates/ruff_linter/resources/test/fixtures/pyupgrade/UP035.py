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
