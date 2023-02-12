# unrecognized-platform-name (PYI008)

Derived from the **flake8-pyi** linter.

## What it does
Check for unrecognized platform names in `sys.platform` checks.

> **Note**
>
> This rule only supports the stub file.

## Why is this bad?
To prevent you from typos, we warn if you use a platform name outside a
small set of known platforms (e.g. "linux" and "win32").

## Example
Use a platform name from the list of known platforms. Currently, the
list of known platforms is: "linux", "win32", "cygwin", "darwin".
```python
if sys.platform == 'win32':
   # Windows specific definitions
else:
   # Posix specific definitions
```