"""
Default projects for ecosystem checks
"""
from ruff_ecosystem.projects import (
    CheckOptions,
    FormatOptions,
    Project,
    Repository,
)

JUPYTER_NOTEBOOK_SELECT = "A,E703,F704,B015,B018,D100"

# TODO(zanieb): Consider exporting this as JSON and loading from there instead
DEFAULT_TARGETS = [
    Project(repo=Repository(owner="DisnakeDev", name="disnake", ref="master")),
    Project(repo=Repository(owner="PostHog", name="HouseWatch", ref="main")),
    Project(repo=Repository(owner="RasaHQ", name="rasa", ref="main")),
    Project(repo=Repository(owner="Snowflake-Labs", name="snowcli", ref="main")),
    Project(repo=Repository(owner="aiven", name="aiven-client", ref="main")),
    Project(repo=Repository(owner="alteryx", name="featuretools", ref="main")),
    Project(
        repo=Repository(owner="apache", name="airflow", ref="main"),
        check_options=CheckOptions(select="ALL"),
    ),
    Project(repo=Repository(owner="aws", name="aws-sam-cli", ref="develop")),
    Project(repo=Repository(owner="bloomberg", name="pytest-memray", ref="main")),
    Project(
        repo=Repository(owner="bokeh", name="bokeh", ref="branch-3.3"),
        check_options=CheckOptions(select="ALL"),
    ),
    Project(repo=Repository(owner="commaai", name="openpilot", ref="master")),
    Project(
        repo=Repository(owner="demisto", name="content", ref="master"),
        format_options=FormatOptions(
            # Syntax errors in this file
            exclude="Packs/ThreatQ/Integrations/ThreatQ/ThreatQ.py"
        ),
    ),
    Project(repo=Repository(owner="docker", name="docker-py", ref="main")),
    Project(repo=Repository(owner="freedomofpress", name="securedrop", ref="develop")),
    Project(repo=Repository(owner="fronzbot", name="blinkpy", ref="dev")),
    Project(repo=Repository(owner="ibis-project", name="ibis", ref="main")),
    Project(repo=Repository(owner="ing-bank", name="probatus", ref="main")),
    Project(repo=Repository(owner="jrnl-org", name="jrnl", ref="develop")),
    Project(repo=Repository(owner="latchbio", name="latch", ref="main")),
    Project(repo=Repository(owner="lnbits", name="lnbits", ref="main")),
    Project(repo=Repository(owner="milvus-io", name="pymilvus", ref="master")),
    Project(repo=Repository(owner="mlflow", name="mlflow", ref="master")),
    Project(repo=Repository(owner="model-bakers", name="model_bakery", ref="main")),
    Project(repo=Repository(owner="pandas-dev", name="pandas", ref="main")),
    Project(repo=Repository(owner="prefecthq", name="prefect", ref="main")),
    Project(repo=Repository(owner="pypa", name="build", ref="main")),
    Project(repo=Repository(owner="pypa", name="cibuildwheel", ref="main")),
    Project(repo=Repository(owner="pypa", name="pip", ref="main")),
    Project(
        repo=Repository(owner="pypa", name="setuptools", ref="main"),
    ),
    Project(repo=Repository(owner="python", name="mypy", ref="master")),
    Project(
        repo=Repository(
            owner="python",
            name="typeshed",
            ref="main",
        ),
        check_options=CheckOptions(select="PYI"),
    ),
    Project(repo=Repository(owner="python-poetry", name="poetry", ref="master")),
    Project(repo=Repository(owner="reflex-dev", name="reflex", ref="main")),
    Project(repo=Repository(owner="rotki", name="rotki", ref="develop")),
    Project(repo=Repository(owner="scikit-build", name="scikit-build", ref="main")),
    Project(
        repo=Repository(owner="scikit-build", name="scikit-build-core", ref="main")
    ),
    # TODO(charlie): Ecosystem check fails in non-preview due to the direct
    # selection of preview rules.
    # Project(
    #     repo=Repository(
    #         owner="sphinx-doc",
    #         name="sphinx",
    #         ref="master",
    #     ),
    #     format_options=FormatOptions(
    #         # Does not contain valid UTF-8
    #         exclude="tests/roots/test-pycode/cp_1251_coded.py"
    #     ),
    # ),
    Project(repo=Repository(owner="spruceid", name="siwe-py", ref="main")),
    Project(repo=Repository(owner="tiangolo", name="fastapi", ref="master")),
    Project(repo=Repository(owner="yandex", name="ch-backup", ref="main")),
    Project(
        repo=Repository(owner="zulip", name="zulip", ref="main"),
        check_options=CheckOptions(select="ALL"),
    ),
    Project(
        repo=Repository(owner="indico", name="indico", ref="master"),
        # Remove once indico removed S401 from their ignore configuration
        config_overrides={
            "lint.ignore": [
                "E226",  # allow omitting whitespace around arithmetic operators
                "E731",
                # allow assigning lambdas (it's useful for single-line functions defined inside other functions)
                "N818",  # not all our exceptions are errors
                "RUF012",  # ultra-noisy and dicts in classvars are very common
                "RUF015",  # not always more readable, and we don't do it for huge lists
                "RUF022",  # autofix messes up out formatting instead of just sorting
                "RUF027",  # also triggers on i18n functions -> too noisy for now
                "D205",  # too many docstrings which have no summary line
                "D301",  # https://github.com/astral-sh/ruff/issues/8696
                "D1",  # we have way too many missing docstrings :(
                "D401",  # too noisy (but maybe useful to go through at some point)
                "D412",  # we do not use section, and in click docstrings those blank lines are useful
                "S101",  # we use asserts outside tests, and do not run python with `-O` (also see B011)
                "S113",  # enforcing timeouts would likely require config in some places - maybe later
                "S311",  # false positives, it does not care about the context
                "S324",  # all our md5/sha1 usages are for non-security purposes
                "S404",  # useless, triggers on *all* subprocess imports
                "S403",  # there's already a warning on using pickle, no need to have one for the import
                "S405",  # we don't use lxml in unsafe ways
                "S603",  # useless, triggers on *all* subprocess calls: https://github.com/astral-sh/ruff/issues/4045
                "S607",  # we trust the PATH to be sane
                "B011",  # we don't run python with `-O` (also see S101)
                "B904",  # possibly useful but too noisy
                "COM812",  # trailing commas on multiline lists are nice, but we have 2.5k violations
                "PIE807",  # `lambda: []` is much clearer for `load_default` in schemas
                "PT004",  # pretty weird + not a pytest convention: https://github.com/astral-sh/ruff/issues/8796
                "PT005",  # ^ likewise
                "PT011",  # very noisy
                "PT015",  # nice for tests but not so nice elsewhere
                "PT018",  # ^ likewise
                "SIM102",  # sometimes nested ifs are more readable
                "SIM103",  # sometimes this is more readable (especially when checking multiple conditions)
                "SIM105",  # try-except-pass is faster and people are used to it
                "SIM108",  # noisy ternary
                "SIM114",  # sometimes separate ifs are more readable (especially if they just return a bool)
                "SIM117",  # nested context managers may be more readable
                "PLC0415",  # local imports are there for a reason
                "PLC2701",  # some private imports are needed
                "PLR09",  # too-many-<whatever> is just noisy
                "PLR0913",  # very noisy
                "PLR2004",  # extremely noisy and generally annoying
                "PLR6201",  # sets are faster (by a factor of 10!) but it's noisy and we're in nanoseconds territory
                "PLR6301",  # extremely noisy and generally annoying
                "PLW0108",  # a lambda often makes it more clear what you actually want
                "PLW1510",  # we often do not care about the status code of commands
                "PLW1514",  # we expect UTF8 environments everywhere
                "PLW1641",  # false positives with SA comparator classes
                "PLW2901",  # noisy and reassigning to the loop var is usually intentional
                "TRY002",  # super noisy, and those exceptions are pretty exceptional anyway
                "TRY003",  # super noisy and also useless w/ werkzeugs http exceptions
                "TRY300",  # kind of strange in many cases
                "TRY301",  # sometimes doing that is actually useful
                "TRY400",  # not all exceptions need exception logging
                "PERF203",  # noisy, false positives, and not applicable for 3.11+
                "FURB113",  # less readable
                "FURB140",  # less readable and actually slower in 3.12+
            ]
        },
    ),
    # Jupyter Notebooks
    Project(
        # fork of `huggingface` without syntax errors in notebooks
        repo=Repository(
            owner="zanieb",
            name="huggingface-notebooks",
            ref="zb/fix-syntax",
        ),
        check_options=CheckOptions(select=JUPYTER_NOTEBOOK_SELECT),
        config_overrides={"include": ["*.ipynb"]},
    ),
    Project(
        repo=Repository(owner="openai", name="openai-cookbook", ref="main"),
        check_options=CheckOptions(select=JUPYTER_NOTEBOOK_SELECT),
        config_overrides={
            "include": ["*.ipynb"],
            # TODO(charlie): Re-enable after fixing typo.
            "exclude": [
                "examples/dalle/Image_generations_edits_and_variations_with_DALL-E.ipynb"
            ],
        },
    ),
]
