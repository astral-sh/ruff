del x.__debug__
del x.y, x.__debug__, z.a
x.__debug__ = 1
x.y, x.__debug__, z.a = 1, 2, 3
