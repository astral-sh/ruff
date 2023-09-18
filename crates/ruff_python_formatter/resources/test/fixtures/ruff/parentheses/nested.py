a1 = f(  # 1
    g(  # 2
    )
)
a2 = f(  # 1
    g(  # 2
        x
    )
)
a3 = f(
    (
        #
        ()
    )
)


call(
  a,
  b,
  [  # Empty because of
  ]
)

a = a + b + c + d + ( # Hello
    e + f + g
)

a = int(  # type: ignore
    int(  # type: ignore
        int(  # type: ignore
            6
        )
    )
)

# Stability and correctness checks
b1 = () - (  #
)
() - (  #
)
b2 = () - f(  #
)
() - f(  #
)
b3 = (
    #
    ()
)
(
    #
    ()
)
