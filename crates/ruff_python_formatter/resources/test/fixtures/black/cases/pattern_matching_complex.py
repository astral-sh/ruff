# Cases sampled from Lib/test/test_patma.py

# case black_test_patma_098
match x:
    case -0j:
        y = 0
# case black_test_patma_142
match x:
    case bytes(z):
        y = 0
# case black_test_patma_073
match x:
    case 0 if 0:
        y = 0
    case 0 if 1:
        y = 1
# case black_test_patma_006
match 3:
    case 0 | 1 | 2 | 3:
        x = True
# case black_test_patma_049
match x:
    case [0, 1] | [1, 0]:
        y = 0
# case black_check_sequence_then_mapping
match x:
    case [*_]:
        return "seq"
    case {}:
        return "map"
# case black_test_patma_035
match x:
    case {0: [1, 2, {}]}:
        y = 0
    case {0: [1, 2, {}] | True} | {1: [[]]} | {0: [1, 2, {}]} | [] | "X" | {}:
        y = 1
    case []:
        y = 2
# case black_test_patma_107
match x:
    case 0.25 + 1.75j:
        y = 0
# case black_test_patma_097
match x:
    case -0j:
        y = 0
# case black_test_patma_007
match 4:
    case 0 | 1 | 2 | 3:
        x = True
# case black_test_patma_154
match x:
    case 0 if x:
        y = 0
# case black_test_patma_134
match x:
    case {1: 0}:
        y = 0
    case {0: 0}:
        y = 1
    case {**z}:
        y = 2
# case black_test_patma_185
match Seq():
    case [*_]:
        y = 0
# case black_test_patma_063
match x:
    case 1:
        y = 0
    case 1:
        y = 1
# case black_test_patma_248
match x:
    case {"foo": bar}:
        y = bar
# case black_test_patma_019
match (0, 1, 2):
    case [0, 1, *x, 2]:
        y = 0
# case black_test_patma_052
match x:
    case [0]:
        y = 0
    case [1, 0] if x := x[:0]:
        y = 1
    case [1, 0]:
        y = 2
# case black_test_patma_191
match w:
    case [x, y, *_]:
        z = 0
# case black_test_patma_110
match x:
    case -0.25 - 1.75j:
        y = 0
# case black_test_patma_151
match (x,):
    case [y]:
        z = 0
# case black_test_patma_114
match x:
    case A.B.C.D:
        y = 0
# case black_test_patma_232
match x:
    case None:
        y = 0
# case black_test_patma_058
match x:
    case 0:
        y = 0
# case black_test_patma_233
match x:
    case False:
        y = 0
# case black_test_patma_078
match x:
    case []:
        y = 0
    case [""]:
        y = 1
    case "":
        y = 2
# case black_test_patma_156
match x:
    case z:
        y = 0
# case black_test_patma_189
match w:
    case [x, y, *rest]:
        z = 0
# case black_test_patma_042
match x:
    case (0 as z) | (1 as z) | (2 as z) if z == x % 2:
        y = 0
# case black_test_patma_034
match x:
    case {0: [1, 2, {}]}:
        y = 0
    case {0: [1, 2, {}] | False} | {1: [[]]} | {0: [1, 2, {}]} | [] | "X" | {}:
        y = 1
    case []:
        y = 2
# issue 3790
match (X.type, Y):
    case _:
        pass
# issue 3487
match = re.match(r"(?P<grade>LD|MD|HD)(?P<material>AL|SS)", "HDSS")

match (match.group("grade"), match.group("material")):
    case ("MD" | "HD", "SS" as code):
        print("You will get here")
