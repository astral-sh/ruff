from datetime import datetime

date = ""


### Errors

datetime.fromisoformat(date.replace("Z", "+00:00"))
datetime.fromisoformat(date.replace("Z", "-00:" "00"))

datetime.fromisoformat(date[:-1] + "-00")
datetime.fromisoformat(date[:-1:] + "-0000")

datetime.fromisoformat(date.strip("Z") + """+0"""
                                         """0""")
datetime.fromisoformat(date.rstrip("Z") + "+\x30\60" '\u0030\N{DIGIT ZERO}')

datetime.fromisoformat(
    # Preserved
    (  # Preserved
        date
    ).replace("Z", "+00")
)

datetime.fromisoformat(
    (date
     # Preserved
    )
      .
        rstrip("Z"
               # Unsafe
               ) + "-00" # Preserved
)

datetime.fromisoformat(
    (  # Preserved
        date
    ).strip("Z") + "+0000"
)

datetime.fromisoformat(
    (date
     # Preserved
    )
    [  # Unsafe
        :-1
    ] + "-00"
)


# Edge case
datetime.fromisoformat("Z2025-01-01T00:00:00Z".strip("Z") + "+00:00")


### No errors

datetime.fromisoformat(date.replace("Z"))
datetime.fromisoformat(date.replace("Z", "+0000"), foo)
datetime.fromisoformat(date.replace("Z", "-0000"), foo = " bar")

datetime.fromisoformat(date.replace("Z", "-00", lorem = ipsum))
datetime.fromisoformat(date.replace("Z", -0000))

datetime.fromisoformat(date.replace("z", "+00"))
datetime.fromisoformat(date.replace("Z", "0000"))

datetime.fromisoformat(date.replace("Z", "-000"))

datetime.fromisoformat(date.rstrip("Z") + f"-00")
datetime.fromisoformat(date[:-1] + "-00" + '00')

datetime.fromisoformat(date[:-1] * "-00"'00')

datetime.fromisoformat(date[-1:] + "+00")
datetime.fromisoformat(date[-1::1] + "+00")
