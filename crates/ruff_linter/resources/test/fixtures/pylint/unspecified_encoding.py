import io
import sys
import tempfile
import io as hugo
import codecs

# Errors.
open("test.txt")
io.TextIOWrapper(io.FileIO("test.txt"))
hugo.TextIOWrapper(hugo.FileIO("test.txt"))
tempfile.NamedTemporaryFile("w")
tempfile.TemporaryFile("w")
codecs.open("test.txt")
tempfile.SpooledTemporaryFile(0, "w")

# Non-errors.
open("test.txt", encoding="utf-8")
open("test.bin", "wb")
open("test.bin", mode="wb")
open("test.txt", "r", -1, "utf-8")
open("test.txt", mode=sys.argv[1])

def func(*args, **kwargs):
    open(*args)
    open("text.txt", **kwargs)

io.TextIOWrapper(io.FileIO("test.txt"), encoding="utf-8")
io.TextIOWrapper(io.FileIO("test.txt"), "utf-8")
tempfile.TemporaryFile("w", encoding="utf-8")
tempfile.TemporaryFile("w", -1, "utf-8")
tempfile.TemporaryFile("wb")
tempfile.TemporaryFile()
tempfile.NamedTemporaryFile("w", encoding="utf-8")
tempfile.NamedTemporaryFile("w", -1, "utf-8")
tempfile.NamedTemporaryFile("wb")
tempfile.NamedTemporaryFile()
codecs.open("test.txt", encoding="utf-8")
codecs.open("test.bin", "wb")
codecs.open("test.bin", mode="wb")
codecs.open("test.txt", "r", -1, "utf-8")
tempfile.SpooledTemporaryFile(0, "w", encoding="utf-8")
tempfile.SpooledTemporaryFile(0, "w", -1, "utf-8")
tempfile.SpooledTemporaryFile(0, "wb")
tempfile.SpooledTemporaryFile(0, )

open("test.txt",)
open()
open(
    "test.txt",  # comment
)
open(
    "test.txt",
    # comment
)
open(("test.txt"),)
open(
    ("test.txt"),  # comment
)
open(
    ("test.txt"),
    # comment
)

open((("test.txt")),)
open(
    (("test.txt")),  # comment
)
open(
    (("test.txt")),
    # comment
)
