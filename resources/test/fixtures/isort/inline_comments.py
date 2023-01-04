from a.prometheus.metrics import (  # type:ignore[attr-defined]
    TERMINAL_CURRENTLY_RUNNING_TOTAL,
)

from b.prometheus.metrics import (
    TERMINAL_CURRENTLY_RUNNING_TOTAL,  # type:ignore[attr-defined]
)

from c.prometheus.metrics import TERMINAL_CURRENTLY_RUNNING_TOTAL  # type:ignore[attr-defined]

from d.prometheus.metrics import TERMINAL_CURRENTLY_RUNNING_TOTAL, OTHER_RUNNING_TOTAL  # type:ignore[attr-defined]
