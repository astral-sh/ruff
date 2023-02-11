# implicit-namespace-package (INP001)

Derived from the **flake8-no-pep420** linter.

## What it does
Checks for packages that are missing an `__init__.py` file.

## Why is this bad?
Python packages are directories that contain a file named `__init__.py`.
The existence of this file indicates that the directory is a Python
package, and so it can be imported the same way a module can be
imported.

Directories that lack an `__init__.py` file can still be imported, but
they're indicative of a special kind of package, known as a "namespace
package" (see: [PEP 420](https://www.python.org/dev/peps/pep-0420/)).
Namespace packages are less widely used, so a package that lacks an
`__init__.py` file is typically meant to be a regular package, and
the absence of the `__init__.py` file is probably an oversight.

Note that namespace packages can be specified via the
[`namespace-packages`](https://github.com/charliermarsh/ruff#namespace-packages)
configuration option. Adding a namespace package to the configuration
will suppress this violation for a given package.