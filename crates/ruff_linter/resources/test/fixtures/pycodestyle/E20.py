#: E201:1:6
spam( ham[1], {eggs: 2})
#: E201:1:10
spam(ham[ 1], {eggs: 2})
#: E201:1:15
spam(ham[1], { eggs: 2})
#: E201:1:6
spam(	ham[1], {eggs: 2})
#: E201:1:10
spam(ham[	1], {eggs: 2})
#: E201:1:15
spam(ham[1], {	eggs: 2})
#: Okay
spam(ham[1], {eggs: 2})
#:


#: E202:1:23
spam(ham[1], {eggs: 2} )
#: E202:1:22
spam(ham[1], {eggs: 2 })
#: E202:1:11
spam(ham[1 ], {eggs: 2})
#: E202:1:23
spam(ham[1], {eggs: 2}	)
#: E202:1:22
spam(ham[1], {eggs: 2	})
#: E202:1:11
spam(ham[1	], {eggs: 2})
#: Okay
spam(ham[1], {eggs: 2})

result = func(
    arg1='some value',
    arg2='another value',
)

result = func(
    arg1='some value',
    arg2='another value'
)

result = [
    item for item in items
    if item > 5
]
#:


#: E203:1:10
if x == 4 :
    print(x, y)
    x, y = y, x
#: E203:1:10
if x == 4	:
    print(x, y)
    x, y = y, x
#: E203:2:15 E702:2:16
if x == 4:
    print(x, y) ; x, y = y, x
#: E203:2:15 E702:2:16
if x == 4:
    print(x, y)	; x, y = y, x
#: E203:3:13
if x == 4:
    print(x, y)
    x, y = y , x
#: E203:3:13
if x == 4:
    print(x, y)
    x, y = y	, x
#: Okay
if x == 4:
    print(x, y)
    x, y = y, x
a[b1, :] == a[b1, ...]
b = a[:, b1]

#: E203 linebreak before ]
predictions = predictions[
    len(past_covariates) // datamodule.hparams["downsample"] :
]

#: E203 multi whitespace before :
predictions = predictions[
    len(past_covariates) // datamodule.hparams["downsample"]  :
]

#: E203 tab before :
predictions = predictions[
    len(past_covariates) // datamodule.hparams["downsample"]	:
]

#: E203 single whitespace before : with line a comment
predictions = predictions[
    len(past_covariates) // datamodule.hparams["downsample"] :  # Just some comment
]

#: E203 multi whitespace before : with line a comment
predictions = predictions[
    len(past_covariates) // datamodule.hparams["downsample"]  :  # Just some comment
]

#:

#: E201:1:6
spam[ ~ham]

#: Okay
x = [  #
    'some value',
]

# F-strings
f"{ {'a': 1} }"
f"{[ { {'a': 1} } ]}"
f"normal { {f"{ { [1, 2] } }" } } normal"

#: Okay
ham[lower + offset : upper + offset]

#: Okay
ham[(lower + offset) : upper + offset]

#: E203:1:19
{lower + offset : upper + offset}

#: E203:1:19
ham[lower + offset  : upper + offset]

#: Okay
release_lines = history_file_lines[history_file_lines.index('## Unreleased') + 1: -1]

#: Okay
release_lines = history_file_lines[history_file_lines.index('## Unreleased') + 1 : -1]

#: Okay
ham[1:9], ham[1:9:3], ham[:9:3], ham[1::3], ham[1:9:]
ham[lower:upper], ham[lower:upper:], ham[lower::step]
ham[lower+offset : upper+offset]
ham[: upper_fn(x) : step_fn(x)], ham[:: step_fn(x)]
ham[lower + offset : upper + offset]

#: E201:1:5
ham[ : upper]

#: Okay
ham[lower + offset :: upper + offset]

#: Okay
ham[(lower + offset) :: upper + offset]

#: Okay
ham[lower + offset::upper + offset]

#: E203:1:21
ham[lower + offset : : upper + offset]

#: E203:1:20
ham[lower + offset: :upper + offset]

#: E203:1:20
ham[{lower + offset : upper + offset} : upper + offset]

#: Okay
ham[upper:]

#: Okay
ham[upper :]

#: E202:1:12
ham[upper : ]

#: E203:1:10
ham[upper  :]

#: Okay
ham[lower +1 :, "columnname"]

#: E203:1:13
ham[lower + 1  :, "columnname"]

#: Okay
f"{ham[lower +1 :, "columnname"]}"

#: E203:1:13
f"{ham[lower + 1  :, "columnname"]}"

#: Okay: https://github.com/astral-sh/ruff/issues/12023
f"{x = :.2f}"
f"{(x) = :.2f}"
