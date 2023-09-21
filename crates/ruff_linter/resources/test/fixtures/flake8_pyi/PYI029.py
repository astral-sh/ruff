import builtins
from abc import abstractmethod


def __repr__(self) -> str:
    ...


def __str__(self) -> builtins.str:
    ...


def __repr__(self, /, foo) -> str:
    ...


def __repr__(self, *, foo) -> str:
    ...


class ShouldRemoveSingle:
    def __str__(self) -> builtins.str:
        ...


class ShouldRemove:
    def __repr__(self) -> str:
        ...

    def __str__(self) -> builtins.str:
        ...


class NoReturnSpecified:
    def __str__(self):
        ...

    def __repr__(self):
        ...


class NonMatchingArgs:
    def __str__(self, *, extra) -> builtins.str:
        ...

    def __repr__(self, /, extra) -> str:
        ...


class MatchingArgsButAbstract:
    @abstractmethod
    def __str__(self) -> builtins.str:
        ...

    @abstractmethod
    def __repr__(self) -> str:
        ...
