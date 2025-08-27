from typing import reveal_type

class X[XO]:
    x: list[XO]

type Y[W] = list[W]

def y[YOARG](y: Y[YOARG]):
    reveal_type(y)

def x[XOARG](x: X[XOARG]):
    reveal_type(x.x)
