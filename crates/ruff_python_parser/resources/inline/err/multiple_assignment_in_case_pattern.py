match 2:
    case [y, z, y]: ...  # MatchSequence
    case [y, z, *y]: ...  # MatchSequence
    case [y, y, y]: ...  # MatchSequence multiple
    case {1: x, 2: x}: ...  # MatchMapping duplicate pattern
    case {1: x, **x}: ...  # MatchMapping duplicate in **rest
    case Class(x, x): ...  # MatchClass positional
    case Class(y=x, z=x): ...  # MatchClass keyword
    case [x] | {1: x} | Class(y=x, z=x): ...  # MatchOr
    case x as x: ...  # MatchAs
