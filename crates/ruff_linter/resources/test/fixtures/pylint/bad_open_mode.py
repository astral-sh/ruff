import pathlib

NAME = "foo.bar"
open(NAME, "wb")
open(NAME, "w", encoding="utf-8")
open(NAME, "rb")
open(NAME, "x", encoding="utf-8")
open(NAME, "br")
open(NAME, "+r", encoding="utf-8")
open(NAME, "xb")
open(NAME, "rwx")  # [bad-open-mode]
open(NAME, mode="rwx")  # [bad-open-mode]
open(NAME, "rwx", encoding="utf-8")  # [bad-open-mode]
open(NAME, "rr", encoding="utf-8")  # [bad-open-mode]
open(NAME, "+", encoding="utf-8")  # [bad-open-mode]
open(NAME, "xw", encoding="utf-8")  # [bad-open-mode]
open(NAME, "ab+")
open(NAME, "a+b")
open(NAME, "+ab")
open(NAME, "+rUb")
open(NAME, "x+b")
open(NAME, "Ua", encoding="utf-8")  # [bad-open-mode]
open(NAME, "Ur++", encoding="utf-8")  # [bad-open-mode]
open(NAME, "Ut", encoding="utf-8")
open(NAME, "Ubr")

mode = "rw"
open(NAME, mode)

pathlib.Path(NAME).open("wb")
pathlib.Path(NAME).open(mode)
pathlib.Path(NAME).open("rwx")  # [bad-open-mode]
pathlib.Path(NAME).open(mode="rwx")  # [bad-open-mode]
pathlib.Path(NAME).open("rwx", encoding="utf-8")  # [bad-open-mode]

import builtins
builtins.open(NAME, "Ua", encoding="utf-8")
