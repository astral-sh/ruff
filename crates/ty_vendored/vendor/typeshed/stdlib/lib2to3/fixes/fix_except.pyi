"""Fixer for except statements with named exceptions.

The following cases will be converted:

- "except E, T:" where T is a name:

    except E as T:

- "except E, T:" where T is not a name, tuple or list:

        except E as t:
            T = t

    This is done because the target of an "except" clause must be a
    name.

- "except E, T:" where T is a tuple or list literal:

        except E as t:
            T = t.args
"""

from collections.abc import Generator, Iterable
from typing import ClassVar, Literal, TypeVar

from .. import fixer_base
from ..pytree import Base

_N = TypeVar("_N", bound=Base)

def find_excepts(nodes: Iterable[_N]) -> Generator[tuple[_N, _N], None, None]: ...

class FixExcept(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
