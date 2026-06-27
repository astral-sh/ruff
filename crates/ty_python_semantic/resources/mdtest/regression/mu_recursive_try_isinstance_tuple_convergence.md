# Recursive try/isinstance tuple convergence

This is minimized from an Alerta ecosystem timeout. A value assigned inside a `try` block can keep
its previous loop value on the exception path. If that value is narrowed with `isinstance` and then
packed into a tuple that feeds later loop state, fixed-point iteration must fold the growing tuple
chain instead of expanding it one layer per iteration.

This test guards termination rather than a specific inferred display.

```py
from typing import Any

class Alert:
    is_suppressed: bool

class Plugin:
    def take_action(
        self,
        alert: Alert,
        action: str,
        text: str,
        timeout: int | None = None,
    ) -> Any: ...

plugins: list[Plugin]

def process_action(
    alert: Alert,
    action: str,
    text: str,
    timeout: int | None = None,
) -> tuple[Alert, str, str, int | None]:
    updated = None

    for plugin in plugins:
        if alert.is_suppressed:
            break

        try:
            updated = plugin.take_action(alert, action, text, timeout=timeout)
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
