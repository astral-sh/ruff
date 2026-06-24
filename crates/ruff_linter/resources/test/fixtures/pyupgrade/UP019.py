import typing
import typing as Hello
from typing import Text
from typing import Text as Goodbye


def print_word(word: Text) -> None:
    print(word)


def print_second_word(word: typing.Text) -> None:
    print(word)


def print_third_word(word: Hello.Text) -> None:
    print(word)


def print_fourth_word(word: Goodbye) -> None:
    print(word)
    

import typing_extensions
import typing_extensions as TypingExt
from typing_extensions import Text as TextAlias


def print_fifth_word(word: typing_extensions.Text) -> None:
    print(word)


def print_sixth_word(word: TypingExt.Text) -> None:
    print(word)


def print_seventh_word(word: TextAlias) -> None:
    print(word)
