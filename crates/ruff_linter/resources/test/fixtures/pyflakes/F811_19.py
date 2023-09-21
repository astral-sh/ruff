"""If an imported name is redefined by a class statement which also uses that name in the bases list, no warning is emitted."""

from fu import bar


class bar(bar):
    pass
