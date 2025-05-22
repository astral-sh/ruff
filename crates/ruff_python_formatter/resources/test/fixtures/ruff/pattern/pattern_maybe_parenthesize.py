# Patterns that use BestFit should be parenthesized if they exceed the configured line width
# but fit within parentheses.
match x:
    case (
        "averyLongStringThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsPar"
    ):
        pass


match x:
    case (
        b"averyLongStringThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsPa"
    ):
        pass

match x:
    case (
        f"averyLongStringThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsPa"
    ):
        pass


match x:
    case (
        5444444444444444444444444444444444444444444444444444444444444444444444444444444j
    ):
        pass


match x:
    case (
        5444444444444444444444444444444444444444444444444444444444444444444444444444444
    ):
        pass


match x:
    case (
        5.44444444444444444444444444444444444444444444444444444444444444444444444444444
    ):
        pass


match x:
    case (
        averyLongIdentThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsParenth
    ):
        pass


# But they aren't parenthesized when they exceed the line length even parenthesized
match x:
    case "averyLongStringThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsParenthesized":
        pass


match x:
    case b"averyLongStringThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsParenthesized":
        pass

match x:
    case f"averyLongStringThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsParenthesized":
        pass


match x:
    case 54444444444444444444444444444444444444444444444444444444444444444444444444444444444j:
        pass


match x:
    case 5444444444444444444444444444444444444444444444444444444444444444444444444444444444:
        pass


match x:
    case 5.444444444444444444444444444444444444444444444444444444444444444444444444444444444:
        pass


match x:
    case averyLongIdentifierThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsParenthesized:
        pass


# It uses the Multiline layout when there's an alias.
match x:
    case (
        averyLongIdentifierThatGetsParenthesizedOnceItExceedsTheConfiguredLineWidthFitsParenthe as b
    ):
        pass



match x:
    case (
        "an implicit concatenated" "string literal" "in a match case" "that goes over multiple lines"
    ):
        pass


## Patterns ending with a sequence, mapping, class, or parenthesized pattern should break the parenthesized-like pattern first
match x:
    case A | [
        aaaaaa,
        bbbbbbbbbbbbbbbb,
        cccccccccccccccccc,
        ddddddddddddddddddddddddddd,
    ]:
        pass

match x:
    case A | (
        aaaaaa,
        bbbbbbbbbbbbbbbb,
        cccccccccccccccccc,
        ddddddddddddddddddddddddddd,
    ):
        pass


match x:
    case A | {
        "a": aaaaaa,
        "b": bbbbbbbbbbbbbbbb,
        "c": cccccccccccccccccc,
        "d": ddddddddddddddddddddddddddd,
    }:
        pass


match x:
    case A | Class(
        aaaaaa,
        bbbbbbbbbbbbbbbb,
        cccccccccccccccccc,
        ddddddddddddddddddddddddddd,
    ):
        pass



match x:
    case A | (
        aaaaaaaaaaaaaaaaaaa.bbbbbbbbbbbbbbbbbbbbbbb.cccccccccccccccccccccccccccc.ddddddddddddddddddddddd
    ):
        pass


## Patterns starting with a sequence, mapping, class, or parenthesized pattern should break the parenthesized-like pattern first
match x:
    case [
         aaaaaa,
         bbbbbbbbbbbbbbbb,
         cccccccccccccccccc,
         ddddddddddddddddddddddddddd,
     ] | A:
        pass

match x:
    case (
         aaaaaa,
         bbbbbbbbbbbbbbbb,
         cccccccccccccccccc,
         ddddddddddddddddddddddddddd,
     ) | A:
        pass


match x:
    case {
         "a": aaaaaa,
         "b": bbbbbbbbbbbbbbbb,
         "c": cccccccccccccccccc,
         "d": ddddddddddddddddddddddddddd,
         } | A:
        pass


match x:
    case Class(
        aaaaaa,
        bbbbbbbbbbbbbbbb,
        cccccccccccccccccc,
        ddddddddddddddddddddddddddd,
    ):
        pass


## Not for non-parenthesized sequence patterns
match x:
    case (
        (1) | aaaaaaaaaaaaaaaaaaaaaaaaaaaa,
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
        ccccccccccccccccccccccccccccccccc,
    ):
        pass

## Parenthesize patterns that start with a token
match x:
    case (
    A(
        aaaaaaaaaaaaaaaaaaa.bbbbbbbbbbbbbbbbbbbbbbb.cccccccccccccccccccccccccccc.ddddddddddddddddddddddd
    )
    | B
    ):
        pass


## Always use parentheses for implicitly concatenated strings
match x:
    case (
        "implicit"
        "concatenated"
        "string"
        | [aaaaaa, bbbbbbbbbbbbbbbb, cccccccccccccccccc, ddddddddddddddddddddddddddd]
    ):
        pass


match x:
    case (
        b"implicit"
        b"concatenated"
        b"string"
        | [aaaaaa, bbbbbbbbbbbbbbbb, cccccccccccccccccc, ddddddddddddddddddddddddddd]
    ):
        pass


match x:
    case (
         f"implicit"
         "concatenated"
         "string"
         | [aaaaaa, bbbbbbbbbbbbbbbb, cccccccccccccccccc, ddddddddddddddddddddddddddd]
    ):
        pass


## Complex number expressions and unary expressions

match x:
    case 4 - 3j | [
        aaaaaaaaaaaaaaaaaaaaaaaa,
        bbbbbbbbbbbbbbbbbbbbbbbbbbbb,
        cccccccccccccccccccccccccccccccccccccccc,
    ]:
        pass


match x:
    case 4 + 3j | [
        aaaaaaaaaaaaaaaaaaaaaaaa,
        bbbbbbbbbbbbbbbbbbbbbbbbbbbb,
        cccccccccccccccccccccccccccccccccccccccc,
    ]:
        pass


match x:
    case -1 | [
        aaaaaaaaaaaaaaaaaaaaaaaaaaaa,
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
        ccccccccccccccccccccccccccccccccc,
    ]:
        pass



### Parenthesized patterns
match x:
    case (1) | [
        aaaaaaaaaaaaaaaaaaaaaaaaaaaa,
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
        ccccccccccccccccccccccccccccccccc,
    ]:
        pass


match x:
    case ( # comment
         1
     ) | [
        aaaaaaaaaaaaaaaaaaaaaaaaaaaa,
        bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
        ccccccccccccccccccccccccccccccccc,
    ]:
        pass

match a, b:
    case [], []:
        ...
    case [], _:
        ...
    case _, []:
        ...
    case _, _:
        ...

