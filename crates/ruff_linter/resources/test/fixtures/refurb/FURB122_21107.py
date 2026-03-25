# Attribute and subscript loop targets with walrus (version-dependent; #21107).
from types import SimpleNamespace


def _():
    ns = SimpleNamespace(prev=[], last=None)
    with open("file", "r", encoding="utf-8") as src, open("file", "w", encoding="utf-8") as dst:
        for ns.last in src:
            dst.write((ns := SimpleNamespace(prev=[*ns.prev, ns.last], last=None)).prev[-1])


def _():
    cache = {}
    index = 0
    with open("file", "r", encoding="utf-8") as src, open("file", "w", encoding="utf-8") as dst:
        for cache[index] in src:
            dst.write(str(index := 0))
