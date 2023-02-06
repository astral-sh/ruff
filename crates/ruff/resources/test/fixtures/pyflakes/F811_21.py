"""Test: noqa directives."""

from typing_extensions import List, Sequence

# This should ignore both errors.
from typing import (  # noqa: F811
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
    List,  # noqa: F811
    Sequence,  # noqa: F811
)

# This should ignore both errors.
from typing import (
    List,  # noqa
    Sequence,  # noqa
)

# This should ignore the first error.
from typing import (
    List,  # noqa: F811
    Sequence,
)

# This should ignore both errors.
from typing import (  # noqa
    List,
    Sequence,
)

# This should ignore both errors.
from typing import List, Sequence  # noqa: F811

# This should ignore both errors.
from typing import List, Sequence  # noqa


def f():
    # This should ignore both errors.
    from typing import (  # noqa: F811
        List,
        Sequence,
    )
