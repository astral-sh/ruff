# `useless-exception-statement` (`PLW0133`)

```toml
[lint]
preview = true
select = ["PLW0133"]
```

## Imported names do not collide with local exceptions

The custom-exception check resolves the binding referenced by the call. A
qualified or imported name must not be mistaken for an unrelated local
exception with the same final segment.

```py
import http
from http import HTTPStatus as ImportedHTTPStatus


class HTTPStatus(Exception):
    pass


HTTPStatus()  # error: [useless-exception-statement]
http.HTTPStatus(200)
ImportedHTTPStatus(200)
```
