match 2:
    case [y, z, y]: ...  # MatchSequence
    case [y, z, *y]: ...  # MatchSequence
    case [y, y, y]: ...  # MatchSequence multiple
    case {1: x, 2: x}: ...  # MatchMapping duplicate pattern
    case {1: x, **x}: ...  # MatchMapping duplicate in **rest
    case Class(x, x): ...  # MatchClass positional
    case Class(x=1, x=2): ...  # MatchClass keyword
    case [x] | {1: x} | Class(x=1, x=2): ...  # MatchOr
    case x as x: ...  # MatchAs
