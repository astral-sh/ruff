"""Test: noqa directives."""

# This should ignore both errors.
from typing import (  # noqa: F401
    List,
    Sequence,
)

# This should ignore both errors.
from typing import (  # noqa
    List,
    Sequence,
)

# This should ignore both errors.
from typing import (
    List,  # noqa: F401
    Sequence,  # noqa: F401
)

# This should ignore both errors.
from typing import (
    List,  # noqa
    Sequence,  # noqa
)

# This should ignore the first error.
from typing import (
    Mapping,  # noqa: F401
    Union,
)

# This should ignore both errors.
from typing import (  # noqa
    List,
    Sequence,
)

# This should ignore the error, but the inner noqa should be marked as unused.
from typing import (  # noqa: F401
    Optional,  # noqa: F401
)

# This should ignore the error, but the inner noqa should be marked as unused.
from typing import (  # noqa
    Optional,  # noqa: F401
)

# This should ignore the error, but mark F501 as unused.
from typing import (  # noqa: F401
    Dict,  # noqa: F501
)

# This should ignore the error, but mark F501 as unused.
from typing import (  # noqa: F501
    Tuple,  # noqa: F401
)

# This should ignore both errors.
from typing import Any, AnyStr  # noqa: F401

# This should ignore both errors.
from typing import AsyncIterable, AsyncGenerator  # noqa

# This should mark F501 as unused.
from typing import Awaitable, AwaitableGenerator  # noqa: F501
