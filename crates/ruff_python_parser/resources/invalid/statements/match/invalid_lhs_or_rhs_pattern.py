match invalid_lhs_pattern:
    case Foo() + 1j:
        pass
    case x + 2j:
        pass
    case _ + 3j:
        pass
    case (1 | 2) + 4j:
        pass
    case [1, 2] + 5j:
        pass
    case {True: 1} + 6j:
        pass
    case 1j + 2j:
        pass
    case -1j + 2j:
        pass
    case Foo(a as b) + 1j:
        pass

match invalid_rhs_pattern:
    case 1 + Foo():
        pass
    case 2 + x:
        pass
    case 3 + _:
        pass
    case 4 + (1 | 2):
        pass
    case 5 + [1, 2]:
        pass
    case 6 + {True: 1}:
        pass
    case 1 + 2:
        pass
    case 1 + Foo(a as b):
        pass

match invalid_lhs_rhs_pattern:
    case Foo() + Bar():
        pass
