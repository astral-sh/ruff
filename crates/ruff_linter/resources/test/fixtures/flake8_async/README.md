# ASYNC102 reference fixtures

`ASYNC102.py`, `ASYNC102_anyio.py`, `ASYNC102_trio.py`, and
`ASYNC102_except_star.py` are adapted from the corresponding `async102*` files in
[flake8-async](https://github.com/python-trio/flake8-async/tree/c695f61dd9e237375c197f20f3a3eb41885b4eca/tests/eval_files).
The last file comes from `async102_120_py311.py`. Comments referring to ASYNC120
describe that separate upstream rule; these snapshots select only ASYNC102.

The fixtures cover shielding without a timeout, cancellation-catching handlers,
`except*`, `__aexit__`, safe cleanup calls, and nursery/task-group shields. The
upstream diagnostic still mentions a timeout, but the implementation and tests
removed that requirement in 25.5.2. Ruff's diagnostic reflects the implemented
behavior.

Ruff tracks every context-manager item, so the reference expectation for the
second shield in a multi-item `with` has been updated. `ASYNC102_ruff.py` covers
additional Ruff regressions, including semantic import resolution, all
`except*` handlers, nested cleanup without duplicate diagnostics, and conditional
shield assignments. The asyncio-only and no-import fixtures establish that Ruff
requires evidence of Trio or AnyIO, unlike flake8-async's implicit Trio default.

## Reference fixture license

The copied flake8-async fixtures are distributed under the following license:

MIT License

Copyright (c) 2022 Zac Hatfield-Dodds

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
