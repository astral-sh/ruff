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


