# Recursive tuple unpack convergence

This is a reduced regression test from Alerta. The loop-carried `updated` binding can flow through
an exception handler, an `isinstance` narrowing, tuple construction, tuple length narrowing, and
tuple unpacking. This used to keep producing larger finite tuple approximations instead of
converging.

```py
from typing import Any, Iterable, Optional, Tuple

class Alert:
    is_suppressed: bool

class Plugin:
    def take_action(self, alert: Alert, action: str, text: str, **kwargs) -> Any:
        raise NotImplementedError

def routing() -> Tuple[Iterable[Plugin], object]:
    return (), object()

def f(
    alert: Alert,
    action: str,
    text: str,
    timeout: Optional[int] = None,
) -> Tuple[Alert, str, str, Optional[int]]:
    wanted_plugins, wanted_config = routing()

    updated = None
    for plugin in wanted_plugins:
        if alert.is_suppressed:
            break

        try:
            updated = plugin.take_action(alert, action, text, timeout=timeout, config=wanted_config)
        except Exception:
            pass

        if isinstance(updated, Alert):
            updated = updated, action, text, timeout

        if isinstance(updated, tuple):
            if len(updated) == 4:
                alert, action, text, timeout = updated
            elif len(updated) == 3:
                alert, action, text = updated

    return alert, action, text, timeout
```
