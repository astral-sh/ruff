# Subscripts involving type aliases

Aliases are expanded during analysis of subscripts.

```toml
[environment]
python-version = "3.12"
```

```py
from typing_extensions import TypeAlias, Literal

ImplicitTuple = tuple[str, int, int]
PEP613Tuple: TypeAlias = tuple[str, int, int]
type PEP695Tuple = tuple[str, int, int]

ImplicitZero = Literal[0]
PEP613Zero: TypeAlias = Literal[0]
type PEP695Zero = Literal[0]

def f(
    implicit_tuple: ImplicitTuple,
    pep_613_tuple: PEP613Tuple,
    pep_695_tuple: PEP695Tuple,
    implicit_zero: ImplicitZero,
    pep_613_zero: PEP613Zero,
    pep_695_zero: PEP695Zero,
):
    reveal_type(implicit_tuple[:2])  # revealed: tuple[str, int]
    reveal_type(implicit_tuple[implicit_zero])  # revealed: str
    reveal_type(implicit_tuple[pep_613_zero])  # revealed: str
    reveal_type(implicit_tuple[pep_695_zero])  # revealed: str

    reveal_type(pep_613_tuple[:2])  # revealed: tuple[str, int]
    reveal_type(pep_613_tuple[implicit_zero])  # revealed: str
    reveal_type(pep_613_tuple[pep_613_zero])  # revealed: str
    reveal_type(pep_613_tuple[pep_695_zero])  # revealed: str

    reveal_type(pep_695_tuple[:2])  # revealed: tuple[str, int]
    reveal_type(pep_695_tuple[implicit_zero])  # revealed: str
    reveal_type(pep_695_tuple[pep_613_zero])  # revealed: str
    reveal_type(pep_695_tuple[pep_695_zero])  # revealed: str
```
