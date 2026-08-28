# Missing direct dependencies

An installed package is not necessarily a declared dependency. The `missing-direct-dependency` rule
checks imports against the current project's direct dependencies when dependency metadata is
available.

## Without dependency metadata

Enabling the rule has no effect when the package manager has not supplied dependency metadata.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"
```

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`main.py`:

```py
import indirect
```

## Direct dependency declarations

The project declares `direct-dependency`, which provides `direct` and `facade`. It also has
`indirect-distribution` installed, but does not declare that distribution as a direct dependency.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [{ path = "/src", distribution = "app", dependencies = ["direct"] }]

[dependency-metadata.distributions]
app = { name = "app-project" }
direct = { name = "direct-dependency" }
indirect = { name = "indirect-distribution" }

[dependency-metadata.module-owners]
app = ["app"]
direct = ["direct"]
facade = ["direct"]
indirect = ["indirect"]
```

### Plain and aliased imports

Importing a declared dependency is allowed. Imports of an undeclared distribution are reported,
including aliases and imports of its submodules.

`/.venv/<path-to-site-packages>/direct/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/indirect/child.py`:

```py
value = 1
```

`main.py`:

```py
import direct

# snapshot: missing-direct-dependency
import indirect
import indirect as alias  # error: [missing-direct-dependency] "Import of `indirect` requires a direct dependency on `indirect-distribution`"
import indirect.child  # error: [missing-direct-dependency]
from indirect.child import value  # error: [missing-direct-dependency]
```

```snapshot
warning[missing-direct-dependency]: Import of `indirect` requires a direct dependency on `indirect-distribution`
 --> src/main.py:4:8
  |
4 | import indirect
  |        ^^^^^^^^
help: Declare `indirect-distribution` in `project.dependencies` or `project.optional-dependencies` in your `pyproject.toml`
info: See https://docs.astral.sh/uv/concepts/projects/dependencies/
```

### From imports and star imports

Each imported name from an undeclared distribution is reported. Star imports also require a direct
dependency, whether they occur in the same file or another file.

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
first = 1
second = 2
```

`main.py`:

```py
# error: [missing-direct-dependency] "Import of `indirect` requires a direct dependency on `indirect-distribution`"
# error: [missing-direct-dependency]
from indirect import first, second
from indirect import *  # error: [missing-direct-dependency]
```

`star.py`:

```py
from indirect import *  # error: [missing-direct-dependency]
```

### Star imports without exported names

A star import of an empty module still uses its distribution, even though it binds no names.

`/.venv/<path-to-site-packages>/indirect.py`:

```py
```

`main.py`:

```py
from indirect import *  # error: [missing-direct-dependency]
```

### Imports inside functions and classes

Imports in nested scopes require direct dependencies just like imports at module scope. Imports in
functions and classes are reported independently of imports at module scope.

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`main.py`:

```py
def use_dependency():
    import indirect  # error: [missing-direct-dependency]

class UsesDependency:
    import indirect  # error: [missing-direct-dependency]

import indirect  # error: [missing-direct-dependency]
```

`class_first.py`:

```py
class UsesDependency:
    import indirect  # error: [missing-direct-dependency]

def use_dependency():
    import indirect  # error: [missing-direct-dependency]

import indirect  # error: [missing-direct-dependency]
```

### Type-checking and unreachable imports

Imports guarded by `TYPE_CHECKING` do not introduce runtime dependencies. Unreachable imports are
also ignored, including in nested scopes. Neither hides a later runtime import's diagnostic.

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`main.py`:

```py
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import indirect

if False:
    import indirect

def use_dependency():
    if TYPE_CHECKING:
        import indirect
    if False:
        import indirect

import indirect  # error: [missing-direct-dependency]
```

### Stub files

Imports in a stub describe types, not runtime dependencies. They do not require declarations in the
project's runtime dependencies.

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
class Value: ...
```

`main.pyi`:

```pyi
import indirect
from indirect import Value
```

### Unresolved imports and unknown ownership

An import that cannot be resolved receives only `unresolved-import`. A resolved module with no known
distribution owner is not enough evidence to report a missing dependency. Standard-library imports
do not require project dependencies either.

`/.venv/<path-to-site-packages>/unowned.py`:

```py
```

`main.py`:

```py
import indirect  # error: [unresolved-import]
import unowned
import sys
```

### Local modules and self-imports

A first-party module can shadow an installed module with the same name. Imports of that local
module, or of the project itself, do not require another dependency declaration.

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`indirect.py`:

```py
```

`app/__init__.py`:

```py
```

`main.py`:

```py
import indirect
import app
```

### Re-exported values

A declared dependency can expose values implemented by one of its dependencies. Importing the public
value from the declared dependency does not require a direct dependency on its implementation.

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
class Value: ...
```

`/.venv/<path-to-site-packages>/facade/__init__.py`:

```py
from indirect import Value
```

`main.py`:

```py
from facade import Value
```

## Module ownership

Import names can differ from distribution names. Namespace packages can also contain modules from
several distributions, so the most specific known owner determines the dependency to declare.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [{ path = "/src", dependencies = ["core"] }]

[dependency-metadata.distributions]
core = { name = "core-distribution" }
storage = { name = "storage-distribution" }
other = { name = "other-distribution" }
runtime = { name = "runtime-distribution" }
indirect = { name = "indirect-distribution" }

[dependency-metadata.module-owners]
ns = ["storage", "other"]
"ns.core" = ["core"]
"ns.storage" = ["storage"]
"ns.other" = ["other"]
shared = ["storage", "other"]
shared_namespace = ["indirect"]
"shared_namespace.external" = ["indirect"]
typed = ["runtime"]
```

### Namespace children

The namespace itself has no unique owner. Its children do, including children imported with
`from ns import ...`. Distinct missing distributions in one statement receive separate diagnostics.

`/.venv/<path-to-site-packages>/ns/storage/__init__.py`:

```py
value = 1
```

`/.venv/<path-to-site-packages>/ns/other/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/ns/core/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/ns/storage_extra.py`:

```py
```

`main.py`:

```py
import ns
import ns.core

# error: [missing-direct-dependency] "direct dependency on `storage-distribution`"
# error: [missing-direct-dependency] "direct dependency on `other-distribution`"
from ns import storage, other

import ns.storage  # error: [missing-direct-dependency] "direct dependency on `storage-distribution`"
from ns.storage import value  # error: [missing-direct-dependency] "direct dependency on `storage-distribution`"

# `ns.storage` is not a module-name prefix of `ns.storage_extra`.
import ns.storage_extra
```

### Namespaces containing local and installed modules

A namespace can contain both local modules and modules from an installed distribution. Ownership of
the installed part does not imply ownership of the namespace or its local children. Only imports of
the external child require that distribution as a dependency.

`shared_namespace/local.py`:

```py
```

`/.venv/<path-to-site-packages>/shared_namespace/external.py`:

```py
```

`main.py`:

```py
import shared_namespace
from shared_namespace import local
import shared_namespace.local

from shared_namespace import external  # error: [missing-direct-dependency] "direct dependency on `indirect-distribution`"
import shared_namespace.external  # error: [missing-direct-dependency] "direct dependency on `indirect-distribution`"
```

### Shared namespaces with inline stubs

An installed `__init__.pyi` does not change which modules share the namespace at runtime. Imports of
the namespace and its local child remain allowed; imports of its installed child require a
dependency.

`shared_namespace/local.py`:

```py
```

`/.venv/<path-to-site-packages>/shared_namespace/__init__.pyi`:

```pyi
```

`/.venv/<path-to-site-packages>/shared_namespace/external.py`:

```py
```

`main.py`:

```py
import shared_namespace
from shared_namespace import local
import shared_namespace.local

from shared_namespace import external  # error: [missing-direct-dependency] "direct dependency on `indirect-distribution`"
import shared_namespace.external  # error: [missing-direct-dependency] "direct dependency on `indirect-distribution`"
```

### Ambiguous ownership

When multiple distributions claim the same module, the rule does not guess which one to declare.

TODO: When none of the possible owners is an allowed dependency, report the import and list the
candidate distributions in the diagnostic.

`/.venv/<path-to-site-packages>/shared/__init__.py`:

```py
```

`main.py`:

```py
import shared
```

### Runtime distributions with separate stubs

Type checking can resolve an import through a stub package. The runtime module's distribution still
determines the dependency required by a runtime import.

`/.venv/<path-to-site-packages>/typed-stubs/__init__.pyi`:

```pyi
value: int
```

`/.venv/<path-to-site-packages>/typed/__init__.py`:

```py
value = 1
```

`main.py`:

```py
import typed  # error: [missing-direct-dependency] "direct dependency on `runtime-distribution`"
from typed import value  # error: [missing-direct-dependency] "direct dependency on `runtime-distribution`"

reveal_type(value)  # revealed: int
```

### Package stubs for runtime namespaces

An `__init__.pyi` does not make a regular package at runtime. The resulting namespace may include
files from other locations, so importing it does not identify a missing dependency.

`/.venv/<path-to-site-packages>/typed/__init__.pyi`:

```pyi
value: int
```

`main.py`:

```py
import typed
from typed import value

reveal_type(typed.value)  # revealed: int
reveal_type(value)  # revealed: int
```

### Native runtime distributions with visible stubs

For a native runtime module, ty may only resolve a `.pyi` file. Ownership supplied by the package
manager still identifies the runtime dependency, while the stub supplies its types.

`/.venv/<path-to-site-packages>/typed.pyi`:

```pyi
value: int
```

`main.py`:

```py
import typed  # error: [missing-direct-dependency] "direct dependency on `runtime-distribution`"

reveal_type(typed.value)  # revealed: int
```

## Projects and dependency groups

The root project declares `direct-dependency` as a runtime dependency and `development-tool` in a
dependency group. A nested project has its own dependency declarations.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [
    { path = "/src", distribution = "app", dependencies = ["direct"], group-dependencies = ["dev"] },
    { path = "/src/nested", distribution = "nested", dependencies = ["indirect"] },
]

[dependency-metadata.distributions]
app = { name = "app-project" }
nested = { name = "nested-project" }
direct = { name = "direct-dependency" }
dev = { name = "development-tool" }
indirect = { name = "indirect-distribution" }
editable = { name = "editable-distribution", editable-path = "/editable/lib" }

[dependency-metadata.module-owners]
app = ["app"]
nested = ["nested"]
direct = ["direct"]
devtool = ["dev"]
indirect = ["indirect"]
```

### Package code and tests

Package code cannot rely on a dependency group. Tests can use direct group dependencies, but not
packages installed only as their transitive dependencies.

`/.venv/<path-to-site-packages>/direct/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/devtool/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`app/__init__.py`:

```py
import direct
import devtool  # error: [missing-direct-dependency] "direct dependency on `development-tool`"
```

`tests/test_app.py`:

```py
import direct
import devtool
import indirect  # error: [missing-direct-dependency] "direct dependency on `indirect-distribution`"
```

### Nested workspace members

The nearest containing project supplies the dependency declarations. Neither project can borrow the
other's direct dependencies.

`/.venv/<path-to-site-packages>/direct/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`main.py`:

```py
import direct
import indirect  # error: [missing-direct-dependency] "direct dependency on `indirect-distribution`"
```

`nested/main.py`:

```py
import indirect
import direct  # error: [missing-direct-dependency] "direct dependency on `direct-dependency`"
```

### Editable dependencies

An editable distribution can expose a module whose name differs from its distribution name. Its
source path identifies the owner even without an entry in the module-owner map.

`/.venv/<path-to-site-packages>/editable.pth`:

```pth
/editable/lib/src
```

`/editable/lib/src/lib_module/__init__.py`:

```py
```

`main.py`:

```py
import lib_module  # error: [missing-direct-dependency] "direct dependency on `editable-distribution`"
```

### Editable legacy namespaces

Two editable distributions contribute to the same legacy namespace. The project directly depends on
the distribution providing `ns.child`, but not on the distribution providing `ns/__init__.py`.
Importing the child requires only the child's distribution, regardless of the import syntax.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [{ path = "/src", dependencies = ["child"] }]

[dependency-metadata.distributions]
parent = { name = "parent-distribution", editable-path = "/editable/parent" }
child = { name = "child-distribution", editable-path = "/editable/child" }
```

`/.venv/<path-to-site-packages>/_parent.pth`:

```pth
/editable/parent/src
```

`/.venv/<path-to-site-packages>/child.pth`:

```pth
/editable/child/src
```

`/editable/parent/src/ns/__init__.py`:

```py
import pkgutil

__path__ = pkgutil.extend_path(__path__, __name__)
value = 1
```

`/editable/child/src/ns/child.py`:

```py
value = 2
```

`main.py`:

```py
import ns.child
from ns.child import value
from ns import child
```

Importing an attribute of the parent still requires its distribution, even when the same statement
also imports the child.

`attributes.py`:

```py
from ns import child, value  # error: [missing-direct-dependency] "direct dependency on `parent-distribution`"
```

Star imports also require the parent's distribution.

`star.py`:

```py
from ns import *  # error: [missing-direct-dependency] "direct dependency on `parent-distribution`"
```

### Editable source roots also configured explicitly

The `.pth` file identifies package code even when the same directory is configured in `extra-paths`.
Tests outside that source directory can use direct dependency-group dependencies.

```toml
[environment]
python = "/.venv"
extra-paths = ["package-src"]

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [{ path = "/src", distribution = "app", group-dependencies = ["dev"] }]

[dependency-metadata.distributions]
app = { name = "app-project", editable-path = "/src" }
dev = { name = "development-tool" }

[dependency-metadata.module-owners]
devtool = ["dev"]
```

`/.venv/<path-to-site-packages>/app.pth`:

```pth
/src/package-src
```

`/.venv/<path-to-site-packages>/devtool/__init__.py`:

```py
```

`package-src/app/__init__.py`:

```py
import devtool  # error: [missing-direct-dependency] "direct dependency on `development-tool`"
```

`tests/test_app.py`:

```py
import devtool
```

### Flat editable roots

An editable search path covering the whole member also makes its tests and development scripts
importable. Without module ownership, that path does not identify which files belong to the
distribution. Direct group dependencies stay allowed, while undeclared dependencies are still
reported.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [{ path = "/src/member", distribution = "app", group-dependencies = ["dev"] }]

[dependency-metadata.distributions]
app = { name = "app-project", editable-path = "/src/member" }
dev = { name = "development-tool" }
indirect = { name = "indirect-distribution" }

[dependency-metadata.module-owners]
devtool = ["dev"]
indirect = ["indirect"]
```

`/.venv/<path-to-site-packages>/app.pth`:

```pth
/src/member
```

`/.venv/<path-to-site-packages>/devtool/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`member/app/__init__.py`:

```py
import devtool
```

`member/tests/test_app.py`:

```py
import devtool
```

`member/scripts/develop.py`:

```py
import devtool
import indirect  # error: [missing-direct-dependency] "direct dependency on `indirect-distribution`"
```

## Script dependency declarations

A PEP 723 script declares runtime dependencies in its inline `dependencies` list. It can import
`direct-dependency`, but importing the installed `indirect-distribution` requires its own
declaration. Imports guarded by `TYPE_CHECKING` do not count. Each runtime import of an undeclared
dependency is reported.

```toml
[environment]
python = "/.venv"
```

`/.venv/<path-to-site-packages>/direct/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`script.py`:

```py
# /// script
# dependencies = ["direct-dependency"]
# [tool.ty.rules]
# missing-direct-dependency = "warn"
# [tool.ty.dependency-metadata]
# projects = [{ path = "/src/script.py", dependencies = ["direct"] }]
# [tool.ty.dependency-metadata.distributions]
# direct = { name = "direct-dependency" }
# indirect = { name = "indirect-distribution" }
# [tool.ty.dependency-metadata.module-owners]
# direct = ["direct"]
# indirect = ["indirect"]
# ///

from typing import TYPE_CHECKING

import direct

if TYPE_CHECKING:
    import indirect

# snapshot: missing-direct-dependency
import indirect
import indirect as alias  # error: [missing-direct-dependency] "Import of `indirect` requires a direct dependency on `indirect-distribution`"
```

```snapshot
warning[missing-direct-dependency]: Import of `indirect` requires a direct dependency on `indirect-distribution`
  --> src/script.py:23:8
   |
23 | import indirect
   |        ^^^^^^^^
help: Declare `indirect-distribution` in the script's inline `dependencies` metadata
info: See https://docs.astral.sh/uv/guides/scripts/#declaring-script-dependencies
```

## Script and workspace isolation

The project and two scripts have different declarations. An import is allowed only when the file's
own declarations include its distribution.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [{ path = "/src", dependencies = ["project"] }]

[dependency-metadata.distributions]
project = { name = "project-dependency" }
script = { name = "script-dependency" }

[dependency-metadata.module-owners]
project_dep = ["project"]
script_dep = ["script"]
```

`/.venv/<path-to-site-packages>/project_dep/__init__.py`:

```py
```

`/.venv/<path-to-site-packages>/script_dep/__init__.py`:

```py
```

`main.py`:

```py
import project_dep
import script_dep  # error: [missing-direct-dependency] "direct dependency on `script-dependency`"
```

The first script declares only `script-dependency`. The project's declaration of
`project-dependency` does not apply to it.

`first.py`:

```py
# /// script
# dependencies = ["script-dependency"]
# [tool.ty.rules]
# missing-direct-dependency = "warn"
# [tool.ty.dependency-metadata]
# projects = [{ path = "/src/first.py", dependencies = ["script"] }]
# [tool.ty.dependency-metadata.distributions]
# project = { name = "project-dependency" }
# script = { name = "script-dependency" }
# [tool.ty.dependency-metadata.module-owners]
# project_dep = ["project"]
# script_dep = ["script"]
# ///

import script_dep
import project_dep  # error: [missing-direct-dependency] "direct dependency on `project-dependency`"
```

The second script declares only `project-dependency`. It cannot use the first script's declaration
of `script-dependency`.

`second.py`:

```py
# /// script
# dependencies = ["project-dependency"]
# [tool.ty.rules]
# missing-direct-dependency = "warn"
# [tool.ty.dependency-metadata]
# projects = [{ path = "/src/second.py", dependencies = ["project"] }]
# [tool.ty.dependency-metadata.distributions]
# project = { name = "project-dependency" }
# script = { name = "script-dependency" }
# [tool.ty.dependency-metadata.module-owners]
# project_dep = ["project"]
# script_dep = ["script"]
# ///

import project_dep
import script_dep  # error: [missing-direct-dependency] "direct dependency on `script-dependency`"
```

## Dependencies imported by local modules

Script dependency checks currently cover imports in the script itself. They do not check imports in
local modules against the script's declarations, a limitation tracked in
[ty#4417](https://github.com/astral-sh/ty/issues/4417). Here, the project declares `attrs`, but the
script declares no dependencies.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [{ path = "/src", dependencies = ["attrs"] }]

[dependency-metadata.distributions]
attrs = { name = "attrs" }

[dependency-metadata.module-owners]
attrs = ["attrs"]
```

`/.venv/<path-to-site-packages>/attrs/__init__.py`:

```py
```

`b.py`:

```py
import attrs
```

Importing `b` does not report its dependency on `attrs` as missing from the script. Importing
`attrs` directly does report the missing declaration.

`script.py`:

```py
# /// script
# dependencies = []
# [tool.ty.rules]
# missing-direct-dependency = "warn"
# [tool.ty.dependency-metadata]
# projects = [{ path = "/src/script.py" }]
# [tool.ty.dependency-metadata.distributions]
# attrs = { name = "attrs" }
# [tool.ty.dependency-metadata.module-owners]
# attrs = ["attrs"]
# ///

import b
import attrs  # error: [missing-direct-dependency] "Import of `attrs` requires a direct dependency on `attrs`"
```

## Unavailable script metadata

Without metadata for a script's environment, the rule cannot establish module ownership. It skips
that script even when the enclosing project has dependency metadata.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "warn"

[dependency-metadata]
projects = [{ path = "/src" }]

[dependency-metadata.distributions]
indirect = { name = "indirect-distribution" }

[dependency-metadata.module-owners]
indirect = ["indirect"]
```

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`main.py`:

```py
import indirect  # error: [missing-direct-dependency]
```

`script.py`:

```py
# /// script
# dependencies = []
# [tool.ty.rules]
# missing-direct-dependency = "warn"
# ///

import indirect
```

## Disabled rule

Dependency metadata does not make the rule mandatory. It can be disabled through rule selection.

```toml
[environment]
python = "/.venv"

[rules]
missing-direct-dependency = "ignore"

[dependency-metadata]
projects = [{ path = "/src" }]

[dependency-metadata.distributions]
indirect = { name = "indirect-distribution" }

[dependency-metadata.module-owners]
indirect = ["indirect"]
```

`/.venv/<path-to-site-packages>/indirect/__init__.py`:

```py
```

`main.py`:

```py
import indirect
```
