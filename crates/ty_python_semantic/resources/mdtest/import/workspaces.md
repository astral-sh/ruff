# Support for Resolving Imports In Workspaces

Python packages have fairly rigid structures that we rely on when resolving imports and merging
namespace packages or stub packages. These rules go out the window when analyzing some random local
python file in some random workspace, and so we need to be more tolerant of situations that wouldn't
fly in a published package, cases where we're not configured as well as we'd like, or cases where
two projects in a monorepo have conflicting definitions (but we want to analyze both at once).

In practice these tests cover what we call "desperate module resolution" which, when an import
fails, results in us walking up the ancestor directories of the importing file and trying those as
"desperate search-paths".

Currently desperate search-paths are restricted to subdirectories of the first-party search-path
(the directory you're running `ty` in). Currently we only consider one desperate search-path: the
closest ancestor directory containing a `pyproject.toml`. In the future we may want to try every
ancestor `pyproject.toml` or every ancestor directory.

## Invalid Names

While you can't syntactically refer to a module with an invalid name (i.e. one with a `-`, or that
has the same name as a keyword) there are plenty of situations where a module with an invalid name
can be run. For instance `python my-script.py` and `python my-proj/main.py` both work, even though
we might in the course of analyzing the code compute the module name `my-script` or `my-proj.main`.

Also, a sufficiently motivated programmer can technically use `importlib.import_module` which takes
strings and does in fact allow syntactically invalid module names.

### Current File Is Invalid Module Name

Relative and absolute imports should resolve fine in a file that isn't a valid module name (in this
case, it could be imported via `importlib.import_module`).

`tests/my-mod.py`:

```py
# TODO: there should be no errors in this file

# error: [unresolved-import]
from .mod1 import x

# error: [unresolved-import]
from . import mod2
import mod3

reveal_type(x)  # revealed: Unknown
reveal_type(mod2.y)  # revealed: Unknown
reveal_type(mod3.z)  # revealed: int
```

`tests/mod1.py`:

```py
x: int = 1
```

`tests/mod2.py`:

```py
y: int = 2
```

`mod3.py`:

```py
z: int = 2
```

### Current Directory Is Invalid Module Name

If python files are rooted in a directory with an invalid module name and they relatively import
each other, there's probably no coherent explanation for what's going on and it's fine that the
relative import don't resolve (but maybe we could provide some good diagnostics).

This is a case that sufficient desperation might "accidentally" make work, so it's included here as
a canary in the coal mine.

`my-tests/mymod.py`:

```py
# error: [unresolved-import]
from .mod1 import x

# error: [unresolved-import]
from . import mod2
import mod3

reveal_type(x)  # revealed: Unknown
reveal_type(mod2.y)  # revealed: Unknown
reveal_type(mod3.z)  # revealed: int
```

`my-tests/mod1.py`:

```py
x: int = 1
```

`my-tests/mod2.py`:

```py
y: int = 2
```

`mod3.py`:

```py
z: int = 2
```

### Ancestor Directory Is Invalid Module Name

Relative and absolute imports *could* resolve fine in the first-party search-path, even if one of
the ancestor dirs is an invalid module. i.e. in this case we will be inclined to compute module
names like `my-proj.tests.mymod`, but it could be that in practice the user always runs this code
rooted in the `my-proj` directory.

This case is hard for us to detect and handle in a principled way, but two more extreme kinds of
desperation could handle this:

- try every ancestor as a desperate search-path
- try the closest ancestor with an invalid module name as a desperate search-path

The second one is a bit messed up because it could result in situations where someone can get a
worse experience because a directory happened to *not* be invalid as a module name (`myproj` or
`my_proj`).

`my-proj/tests/mymod.py`:

```py
# TODO: it would be *nice* if there were no errors in this file

# error: [unresolved-import]
from .mod1 import x

# error: [unresolved-import]
from . import mod2

# error: [unresolved-import]
import mod3

reveal_type(x)  # revealed: Unknown
reveal_type(mod2.y)  # revealed: Unknown
reveal_type(mod3.z)  # revealed: Unknown
```

`my-proj/tests/mod1.py`:

```py
x: int = 1
```

`my-proj/tests/mod2.py`:

```py
y: int = 2
```

`my-proj/mod3.py`:

```py
z: int = 2
```

### Ancestor Directory Above `pyproject.toml` is invalid

Like the previous tests but with a `pyproject.toml` existing between the invalid name and the python
files. This is an "easier" case in case we use the `pyproject.toml` as a hint about what's going on.

`my-proj/pyproject.toml`:

```text
name = "my_proj"
version = "0.1.0"
```

`my-proj/tests/main.py`:

```py
from .mod1 import x
from . import mod2
import mod3

reveal_type(x)  # revealed: int
reveal_type(mod2.y)  # revealed: int
reveal_type(mod3.z)  # revealed: int
```

`my-proj/tests/mod1.py`:

```py
x: int = 1
```

`my-proj/tests/mod2.py`:

```py
y: int = 2
```

`my-proj/mod3.py`:

```py
z: int = 2
```

## Multiple Projects

It's common for a monorepo to define many separate projects that may or may not depend on each other
and are stitched together with a package manager like `uv` or `poetry`, often as editables. In this
case, especially when running as an LSP, we want to be able to analyze all of the projects at once,
allowing us to reuse results between projects, without getting confused about things that only make
sense when analyzing the project separately.

The following tests will feature two projects, `a` and `b` where the "real" packages are found under
`src/` subdirectories (and we've been configured to understand that), but each project also contains
other python files in their roots or subdirectories that contains python files which relatively
import each other and also absolutely import the main package of the project. All of these imports
*should* resolve.

Often the fact that there is both an `a` and `b` project seemingly won't matter, but many possible
solutions will misbehave under these conditions, as e.g. if both define a `main.py` and test code
has `import main`, we need to resolve each project's main as appropriate.

One key hint we will have in these situations is the existence of a `pyproject.toml`, so the
following examples include them in case they help.

### Tests Directory With Overlapping Names

Here we have fairly typical situation where there are two projects `aproj` and `bproj` where the
"real" packages are found under `src/` subdirectories, but each project also contains a `tests/`
directory that contains python files which relatively import each other and also absolutely import
the package they test. All of these imports *should* resolve.

```toml
[environment]
# Setup a venv with editables for aproj/src/ and bproj/src/
python = "/.venv"
```

`/.venv/pyvenv.cfg`:

```cfg
home = /do/re/mi//cpython-3.13.2-macos-aarch64-none/bin
```

`/do/re/mi//cpython-3.13.2-macos-aarch64-none/bin/python`:

```text
```

`/.venv/<path-to-site-packages>/a.pth`:

```pth
aproj/src/
```

`/.venv/<path-to-site-packages>/b.pth`:

```pth
bproj/src/
```

`aproj/tests/test1.py`:

```py
from .setup import x
from . import setup
from a import y
import a

reveal_type(x)  # revealed: int
reveal_type(setup.x)  # revealed: int
reveal_type(y)  # revealed: int
reveal_type(a.y)  # revealed: int
```

`aproj/tests/setup.py`:

```py
x: int = 1
```

`aproj/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`aproj/src/a/__init__.py`:

```py
y: int = 10
```

`bproj/tests/test1.py`:

```py
from .setup import x
from . import setup
from b import y
import b

reveal_type(x)  # revealed: str
reveal_type(setup.x)  # revealed: str
reveal_type(y)  # revealed: str
reveal_type(b.y)  # revealed: str
```

`bproj/tests/setup.py`:

```py
x: str = "2"
```

`bproj/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`bproj/src/b/__init__.py`:

```py
y: str = "20"
```

### Tests Directory With Ambiguous Project Directories Via Editables

The same situation as the previous test but instead of the project `a` being in a directory `aproj`
to disambiguate, we now need to avoid getting confused about whether `a/` or `a/src/a/` is the
package `a` while still resolving imports.

Unfortunately this is a quite difficult square to circle as `a/` is a namespace package of `a` and
`a/src/a/` is a regular package of `a`. **This is a very bad situation you're not supposed to ever
create, and we are now very sensitive to precise search-path ordering.**

Here the use of editables means that `a/` has higher priority than `a/src/a/`.

Somehow this results in `a/tests/test1.py` being able to resolve `.setup` but not `.`.

My best guess is that in this state we can resolve regular modules in `a/tests/` but not namespace
packages because we have some extra validation for namespace packages conflicted by regular
packages, but that validation isn't applied when we successfully resolve a submodule of the
namespace package.

In this case, as we find that `a/tests/test1.py` matches on the first-party path as `a.tests.test1`
and is syntactically valid. We then resolve `a.tests.test1` and because the namespace package
(`/a/`) comes first we succeed. We then syntactically compute `.` to be `a.tests`.

When we go to lookup `a.tests.setup`, whatever grace that allowed `a.tests.test1` to resolve still
works so it resolves too. However when we try to resolve `a.tests` on its own some additional
validation rejects the namespace package conflicting with the regular package.

```toml
[environment]
# Setup a venv with editables for a/src/ and b/src/
python = "/.venv"
```

`/.venv/pyvenv.cfg`:

```cfg
home = /do/re/mi//cpython-3.13.2-macos-aarch64-none/bin
```

`/do/re/mi//cpython-3.13.2-macos-aarch64-none/bin/python`:

```text
```

`/.venv/<path-to-site-packages>/a.pth`:

```pth
a/src/
```

`/.venv/<path-to-site-packages>/b.pth`:

```pth
b/src/
```

`a/tests/test1.py`:

```py
# TODO: there should be no errors in this file.

from .setup import x

# error: [unresolved-import]
from . import setup
from a import y
import a

reveal_type(x)  # revealed: int
reveal_type(setup.x)  # revealed: Unknown
reveal_type(y)  # revealed: int
reveal_type(a.y)  # revealed: int
```

`a/tests/setup.py`:

```py
x: int = 1
```

`a/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`a/src/a/__init__.py`:

```py
y: int = 10
```

`b/tests/test1.py`:

```py
# TODO: there should be no errors in this file

from .setup import x

# error: [unresolved-import]
from . import setup
from b import y
import b

reveal_type(x)  # revealed: str
reveal_type(setup.x)  # revealed: Unknown
reveal_type(y)  # revealed: str
reveal_type(b.y)  # revealed: str
```

`b/tests/setup.py`:

```py
x: str = "2"
```

`b/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`b/src/b/__init__.py`:

```py
y: str = "20"
```

### Tests Directory With Ambiguous Project Directories Via `extra-paths`

The same situation as the previous test but instead of using editables we use `extra-paths` which
have higher priority than the first-party search-path. Thus, `/a/src/a/` is always seen before
`/a/`.

In this case everything works well because the namespace package `a.tests` (`a/tests/`) is
completely hidden by the regular package `a` (`a/src/a/`) and so we immediately enter desperate
resolution and use the now-unambiguous namespace package `tests`.

```toml
[environment]
extra-paths = ["a/src/", "b/src/"]
```

`a/tests/test1.py`:

```py
from .setup import x
from . import setup
from a import y
import a

reveal_type(x)  # revealed: int
reveal_type(setup.x)  # revealed: int
reveal_type(y)  # revealed: int
reveal_type(a.y)  # revealed: int
```

`a/tests/setup.py`:

```py
x: int = 1
```

`a/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`a/src/a/__init__.py`:

```py
y: int = 10
```

`b/tests/test1.py`:

```py
from .setup import x
from . import setup
from b import y
import b

reveal_type(x)  # revealed: str
reveal_type(setup.x)  # revealed: str
reveal_type(y)  # revealed: str
reveal_type(b.y)  # revealed: str
```

`b/tests/setup.py`:

```py
x: str = "2"
```

`b/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`b/src/b/__init__.py`:

```py
y: str = "20"
```

### Tests Directory Absolute Importing `main.py`

Here instead of defining packages we have a couple simple applications with a `main.py` and tests
that `import main` and expect that to work.

`a/tests/test1.py`:

```py
from .setup import x
from . import setup

from main import y
import main

reveal_type(x)  # revealed: int
reveal_type(setup.x)  # revealed: int
reveal_type(y)  # revealed: int
reveal_type(main.y)  # revealed: int
```

`a/tests/setup.py`:

```py
x: int = 1
```

`a/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`a/main.py`:

```py
y: int = 10
```

`b/tests/test1.py`:

```py
from .setup import x
from . import setup

from main import y
import main

reveal_type(x)  # revealed: str
reveal_type(setup.x)  # revealed: str
reveal_type(y)  # revealed: str
reveal_type(main.y)  # revealed: str
```

`b/tests/setup.py`:

```py
x: str = "2"
```

`b/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`b/main.py`:

```py
y: str = "20"
```

### `main.py` absolute importing private package

In this case each project has a `main.py` that defines a "private" `utils` package and absolute
imports it.

`a/main.py`:

```py
from utils import x
import utils

reveal_type(x)  # revealed: int
reveal_type(utils.x)  # revealed: int
```

`a/utils/__init__.py`:

```py
x: int = 1
```

`a/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```

`b/main.py`:

```py
from utils import x
import utils

reveal_type(x)  # revealed: str
reveal_type(utils.x)  # revealed: str
```

`b/utils/__init__.py`:

```py
x: str = "2"
```

`b/pyproject.toml`:

```text
name = "a"
version = "0.1.0"
```
