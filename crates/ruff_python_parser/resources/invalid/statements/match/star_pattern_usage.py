# Star pattern is only allowed inside a sequence pattern
match subject:
    case *_:
        pass
    case *_ as x:
        pass
    case *foo:
        pass
    case *foo | 1:
        pass
    case 1 | *foo:
        pass
    case Foo(*_):
        pass
    case Foo(x=*_):
        pass
    case {*_}:
        pass
    case {*_: 1}:
        pass
    case {None: *_}:
        pass
    case 1 + *_:
        pass
