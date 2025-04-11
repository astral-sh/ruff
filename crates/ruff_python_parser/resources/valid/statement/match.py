# Cases sampled from Lib/test/test_patma.py

# case test_patma_098
match x:
    case -0j:
        y = 0
# case test_patma_142
match x:
    case bytes(z):
        y = 0
# case test_patma_073
match x:
    case 0 if 0:
        y = 0
    case 0 if 1:
        y = 1
# case test_patma_006
match 3:
    case 0 | 1 | 2 | 3:
        x = True
# case test_patma_049
match x:
    case [0, 1] | [1, 0]:
        y = 0
# case black_check_sequence_then_mapping
match x:
    case [*_]:
        return "seq"
    case {}:
        return "map"
# case test_patma_035
match x:
    case {0: [1, 2, {}]}:
        y = 0
    case {0: [1, 2, {}] | True} | {1: [[]]} | {0: [1, 2, {}]} | [] | "X" | {}:
        y = 1
    case []:
        y = 2
# case test_patma_107
match x:
    case 0.25 + 1.75j:
        y = 0
# case test_patma_097
match x:
    case -0j:
        y = 0
# case test_patma_007
match 4:
    case 0 | 1 | 2 | 3:
        x = True
# case test_patma_154
match x:
    case 0 if x:
        y = 0
# case test_patma_134
match x:
    case {1: 0}:
        y = 0
    case {0: 0}:
        y = 1
    case {**z}:
        y = 2
# case test_patma_185
match Seq():
    case [*_]:
        y = 0
# case test_patma_063
match x:
    case 1:
        y = 0
    case 1:
        y = 1
# case test_patma_248
match x:
    case {"foo": bar}:
        y = bar
# case test_patma_019
match (0, 1, 2):
    case [0, 1, *x, 2]:
        y = 0
# case test_patma_052
match x:
    case [0]:
        y = 0
    case [1, 0] if (x := x[:0]):
        y = 1
    case [1, 0]:
        y = 2
# case test_patma_191
match w:
    case [x, y, *_]:
        z = 0
# case test_patma_110
match x:
    case -0.25 - 1.75j:
        y = 0
# case test_patma_151
match (x,):
    case [y]:
        z = 0
# case test_patma_114
match x:
    case A.B.C.D:
        y = 0
# case test_patma_232
match x:
    case None:
        y = 0
# case test_patma_058
match x:
    case 0:
        y = 0
# case test_patma_233
match x:
    case False:
        y = 0
# case test_patma_078
match x:
    case []:
        y = 0
    case [""]:
        y = 1
    case "":
        y = 2
# case test_patma_156
match x:
    case z:
        y = 0
# case test_patma_189
match w:
    case [x, y, *rest]:
        z = 0
# case test_patma_042
match x:
    case (0 as z) | (1 as z) | (2 as z) if z == x % 2:
        y = 0
# case test_patma_034
match x:
    case {0: [1, 2, {}]}:
        y = 0
    case {0: [1, 2, {}] | False} | {1: [[]]} | {0: [1, 2, {}]} | [] | "X" | {}:
        y = 1
    case []:
        y = 2
# case test_patma_123
match (0, 1, 2):
    case 0, *x:
        y = 0
# case test_patma_126
match (0, 1, 2):
    case *x, 2,:
        y = 0
# case test_patma_151
match x,:
    case y,:
        z = 0
# case test_patma_152
match w, x:
    case y, z:
        v = 0
# case test_patma_153
match w := x,:
    case y as v,:
        z = 0

match x:
    # F-strings aren't allowed as patterns but it's a soft syntax error in Python.
    case f"{y}":
        pass
match {"test": 1}:
    case {
        **rest,
    }:
        print(rest)
match {"label": "test"}:
    case {
        "label": str() | None as label,
    }:
        print(label)
match x:
    case [0, 1,]:
        y = 0
match x:
    case (0, 1,):
        y = 0
match x:
    case (0,):
        y = 0
match x,:
    case z:
        pass
match x, y:
    case z:
        pass
match x, y,:
    case z:
        pass

# PatternMatchSingleton
match x:
    case None:
        ...
    case True:
        ...
    case False:
        ...

# PatternMatchValue
match x:
    case a.b:
        ...
    case a.b.c:
        ...
    case '':
        ...
    case b'':
        ...
    case 1:
        ...
    case 1.0:
        ...
    case 1.0J:
        ...
    case 1 + 1j:
        ...
    case -1:
        ...
    case -1.:
        ...
    case -0b01:
        ...
    case (1):
        ...

# PatternMatchOr
match x:
    case 1 | 2:
        ...
    case '' | 1.1 | -1 | 1 + 1j | a.b:
        ...

# PatternMatchAs
match x:
    case a:
        ...
match x:
    case a as b:
        ...
match x:
    case 1 | 2 as two:
        ...
    case 1 + 3j as sum:
        ...
    case a.b as ab:
        ...
    case _ as x:
        ...
match x:
    case _:
        ...

# PatternMatchSequence
match x:
    case 1, 2, 3:
        ...
    case (1, 2, 3,):
        ...
    case (1 + 2j, a, None, a.b):
        ...
    case (1 as X, b) as S:
        ...
    case [1, 2, 3 + 1j]:
        ...
    case ([1,2], 3):
        ...
    case [1]:
        ...

# PatternMatchStar
match x:
    case *a,:
        ...
    case *_,:
        ...
    case [1, 2, *rest]:
        ...
    case (*_, 1, 2):
        ...

# PatternMatchClass
match x:
    case Point():
        ...
    case a.b.Point():
        ...
    case Point2D(x=0):
        ...
    case Point2D(x=0, y=0,):
        ...
    case Point2D(0, 1):
        ...
    case Point2D([0, 1], y=1):
        ...
    case Point2D(x=[0, 1], y=1):
        ...

# PatternMatchMapping
match x := b:
    case {1: _}:
        ...
    case {'': a, None: (1, 2), **rest}:
        ...

# Pattern guard
match y:
    case a if b := c: ...
    case e if  1 < 2: ...

# `match` as an identifier
match *a + b, c   # ((match * a) + b), c
match *(a + b), c   # (match * (a + b)), c
match (*a + b, c)   # match ((*(a + b)), c)
match -a * b + c   # (match - (a * b)) + c
match -(a * b) + c   # (match - (a * b)) + c
match (-a) * b + c   # (match (-(a * b))) + c
match ().a   # (match()).a
match (()).a   # (match(())).a
match ((),).a   # (match(())).a
match [a].b   # (match[a]).b
match [a,].b   # (match[(a,)]).b  (not (match[a]).b)
match [(a,)].b   # (match[(a,)]).b
match()[a:
    b]  # (match())[a: b]
if match := 1: pass
match match:
    case 1: pass
    case 2:
        pass
match = lambda query: query == event
print(match(12))
