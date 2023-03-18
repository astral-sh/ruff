import os

replacements = [
    ("IndentationContainsTabs", "TabIndentation"),
    ("indentation_contains_tabs", "tab_indentation"),
    ("NoNewLineAtEndOfFile", "MissingNewlineAtEndOfFile"),
    ("no_new_line_at_end_of_file", "missing_newline_at_end_of_file"),
    ("BlankLineContainsWhitespace", "BlankLineWithWhitespace"),
    ("blank_line_contains_whitespace", "blank_line_with_whitespace"),
    ("ImportStar", "UndefinedLocalWithImportStar"),
    ("ImportStarNotPermitted", "NestedImportStarUsage"),
    ("TwoStarredExpressions", "MultipleStarredExpressions"),
    ("UsedPriorGlobalDeclaration", "UsePriorToGlobalDeclaration"),
    ("ConsiderUsingFromImport", "ManualFromImport"),
    ("ConsiderMergingIsinstance", "RepeatedIsinstanceCalls"),
    ("ConsiderUsingSysExit", "SysExitAlias"),
    ("FunctionCallArgumentDefault", "FunctionCallInDefaultArgument"),
    ("PrintFound", "Print"),
    ("PPrintFound", "PPrint"),
    ("SysVersionSlice3Referenced", "SysVersionSlice3"),
    ("SysVersion2Referenced", "SysVersion2"),
    ("SysVersionInfo0Eq3Referenced", "SysVersionInfo0Eq3"),
    ("SixPY3Referenced", "SixPY3"),
    ("SysVersion0Referenced", "SysVersion0"),
    ("SysVersionSlice1Referenced", "SysVersionSlice1"),
    ("ManualDictLookup", "IfElseBlockInsteadOfDictLookup"),
    ("UseTernaryOperator", "IfElseBlockInsteadOfIfExp"),
    ("UseCapitalEnvironmentVariables", "UncapitalizedEnvironmentVariables"),
    ("KeyInDict", "InDictKeys"),
    ("DictGetWithDefault", "IfElseBlockInsteadOfDictGet"),
    ("RewriteUnicodeLiteral", "UnicodeKindPrefix"),
    ("RewriteMockImport", "DeprecatedMockImport"),
    ("RewriteYieldFrom", "YieldInForLoop"),
    ("FunctoolsCache", "LRUCacheWithMaxsizeNone"),
    ("ImportReplacements", "DeprecatedImport"),
    ("PublicModule", "UndocumentedPublicModule"),
    ("PublicClass", "UndocumentedPublicClass"),
    ("PublicMethod", "UndocumentedPublicMethod"),
    ("PublicFunction", "UndocumentedPublicFunction"),
    ("PublicPackage", "UndocumentedPublicPackage"),
    ("MagicMethod", "UndocumentedMagicMethod"),
    ("PublicNestedClass", "UndocumentedPublicNestedClass"),
    ("PublicInit", "UndocumentedPublicInit"),
    ("NoUnderIndentation", "UnderIndentation"),
    ("NoOverIndentation", "OverIndentation"),
    ("NoSurroundingWhitespace", "SurroundingWhitespace"),
    ("NoBlankLineBeforeClass", "BlankLineBeforeClass"),
    ("BlankLineAfterSection", "NoBlankLineAfterSection"),
    ("BlankLineBeforeSection", "NoBlankLineBeforeSection"),
    ("NoBlankLinesBetweenHeaderAndContent", "BlankLinesBetweenHeaderAndContent"),
    ("NoEval", "Eval"),
    ("UseOfInplaceArgument", "PandasUseOfInplaceArgument"),
    ("UseOfDotIsNull", "PandasUseOfDotIsNull"),
    ("UseOfDotNotNull", "PandasUseOfDotNotNull"),
    ("UseOfDotIx", "PandasUseOfDotIx"),
    ("UseOfDotAt", "PandasUseOfDotAt"),
    ("UseOfDotIat", "PandasUseOfDotIat"),
    ("UseOfDotPivotOrUnstack", "PandasUseOfDotPivotOrUnstack"),
    ("UseOfDotValues", "PandasUseOfDotValues"),
    ("UseOfDotReadTable", "PandasUseOfDotReadTable"),
    ("UseOfDotStack", "PandasUseOfDotStack"),
    ("UseOfPdMerge", "PandasUseOfPdMerge"),
    ("DfIsABadVariableName", "PandasDfVariableName"),
    ("PrefixTypeParams", "UnprefixedTypeParam"),
    ("TypedArgumentSimpleDefaults", "TypedArgumentDefaultInStub"),
    ("ArgumentSimpleDefaults", "ArgumentDefaultInStub"),
    ("IncorrectFixtureParenthesesStyle", "PytestFixtureIncorrectParenthesesStyle"),
    ("FixturePositionalArgs", "PytestFixturePositionalArgs"),
    ("ExtraneousScopeFunction", "PytestExtraneousScopeFunction"),
    ("MissingFixtureNameUnderscore", "PytestMissingFixtureNameUnderscore"),
    ("IncorrectFixtureNameUnderscore", "PytestIncorrectFixtureNameUnderscore"),
    ("ParametrizeNamesWrongType", "PytestParametrizeNamesWrongType"),
    ("ParametrizeValuesWrongType", "PytestParametrizeValuesWrongType"),
    ("PatchWithLambda", "PytestPatchWithLambda"),
    ("UnittestAssertion", "PytestUnittestAssertion"),
    ("RaisesWithoutException", "PytestRaisesWithoutException"),
    ("RaisesTooBroad", "PytestRaisesTooBroad"),
    ("RaisesWithMultipleStatements", "PytestRaisesWithMultipleStatements"),
    ("IncorrectPytestImport", "PytestIncorrectPytestImport"),
    ("AssertAlwaysFalse", "PytestAssertAlwaysFalse"),
    ("FailWithoutMessage", "PytestFailWithoutMessage"),
    ("AssertInExcept", "PytestAssertInExcept"),
    ("CompositeAssertion", "PytestCompositeAssertion"),
    ("FixtureParamWithoutValue", "PytestFixtureParamWithoutValue"),
    ("DeprecatedYieldFixture", "PytestDeprecatedYieldFixture"),
    ("FixtureFinalizerCallback", "PytestFixtureFinalizerCallback"),
    ("UselessYieldFixture", "PytestUselessYieldFixture"),
    ("IncorrectMarkParenthesesStyle", "PytestIncorrectMarkParenthesesStyle"),
    ("UnnecessaryAsyncioMarkOnFixture", "PytestUnnecessaryAsyncioMarkOnFixture"),
    ("ErroneousUseFixturesOnFixture", "PytestErroneousUseFixturesOnFixture"),
    ("UseFixturesWithoutParameters", "PytestUseFixturesWithoutParameters"),
    ("DupeClassFieldDefinitions", "DuplicateClassFieldDefinition"),
    ("PreferUniqueEnums", "NonUniqueEnums"),
    ("PreferListBuiltin", "ReimplementedListBuiltin"),
    ("SingleStartsEndsWith", "MultipleStartsEndsWith"),
    ("TrailingCommaMissing", "MissingTrailingComma"),
    ("TrailingCommaOnBareTupleProhibited", "TrailingCommaOnBareTuple"),
    ("TrailingCommaProhibited", "ProhibitedTrailingComma"),
    ("ShebangPython", "ShebangMissingPython"),
    ("ShebangWhitespace", "ShebangLeadingWhitespace"),
    ("ShebangNewline", "ShebangNotFirstLine"),
    (
        "UnpackInsteadOfConcatenatingToCollectionLiteral",
        "CollectionLiteralConcatenation",
    ),
    ("NullableModelStringField", "DjangoNullableModelStringField"),
    ("LocalsInRenderFunction", "DjangoLocalsInRenderFunction"),
    ("ExcludeWithModelForm", "DjangoExcludeWithModelForm"),
    ("AllWithModelForm", "DjangoAllWithModelForm"),
    ("ModelWithoutDunderStr", "DjangoModelWithoutDunderStr"),
    ("NonLeadingReceiverDecorator", "DjangoNonLeadingReceiverDecorator"),
]


def replace_in_file(filepath):
    with open(filepath, "r") as f:
        filetext = f.read()

    # Replace all occurrences of "IndentationContainsTabs" with "TabIndentation"
    for old, new in replacements:
        filetext = filetext.replace(old, new)

    with open(filepath, "w") as f:
        f.write(filetext)


# Walk through all files with the specified extensions in the current directory and its
# subdirectories
for root, dirs, files in os.walk("."):
    for filename in files:
        if filename.endswith((".rs", ".snap")):
            filepath = os.path.join(root, filename)
            replace_in_file(filepath)
