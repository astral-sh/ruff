from .models import Repository, CheckOptions, Target

# TODO: Consider exporting this as JSON instead for consistent setup
DEFAULT_TARGETS = [
    # Target(repo=Repository(owner="DisnakeDev", name="disnake", branch="master")),
    # Target(repo=Repository(owner="PostHog", name="HouseWatch", branch="main")),
    # Target(repo=Repository(owner="RasaHQ", name="rasa", branch="main")),
    # Target(repo=Repository(owner="Snowflake-Labs", name="snowcli", branch="main")),
    # Target(repo=Repository(owner="aiven", name="aiven-client", branch="main")),
    # Target(repo=Repository(owner="alteryx", name="featuretools", branch="main")),
    # Target(
    #     repo=Repository(owner="apache", name="airflow", branch="main"),
    #     check_options=CheckOptions(select="ALL"),
    # ),
    # Target(repo=Repository(owner="aws", name="aws-sam-cli", branch="develop")),
    # Target(repo=Repository(owner="bloomberg", name="pytest-memray", branch="main")),
    # Target(
    #     repo=Repository(owner="bokeh", name="bokeh", branch="branch-3.3"),
    #     check_options=CheckOptions(select="ALL"),
    # ),
    # Target(repo=Repository(owner="commaai", name="openpilot", branch="master")),
    # Target(repo=Repository(owner="demisto", name="content", branch="master")),
    # Target(repo=Repository(owner="docker", name="docker-py", branch="main")),
    # Target(
    #     repo=Repository(owner="freedomofpress", name="securedrop", branch="develop")
    # ),
    # Target(repo=Repository(owner="fronzbot", name="blinkpy", branch="dev")),
    # Target(repo=Repository(owner="ibis-project", name="ibis", branch="master")),
    # Target(repo=Repository(owner="ing-bank", name="probatus", branch="main")),
    # Target(repo=Repository(owner="jrnl-org", name="jrnl", branch="develop")),
    # Target(repo=Repository(owner="latchbio", name="latch", branch="main")),
    # Target(repo=Repository(owner="lnbits", name="lnbits", branch="main")),
    # Target(repo=Repository(owner="milvus-io", name="pymilvus", branch="master")),
    # Target(repo=Repository(owner="mlflow", name="mlflow", branch="master")),
    # Target(repo=Repository(owner="model-bakers", name="model_bakery", branch="main")),
    # Target(repo=Repository(owner="pandas-dev", name="pandas", branch="main")),
    # Target(repo=Repository(owner="prefecthq", name="prefect", branch="main")),
    # Target(repo=Repository(owner="pypa", name="build", branch="main")),
    # Target(repo=Repository(owner="pypa", name="cibuildwheel", branch="main")),
    # Target(repo=Repository(owner="pypa", name="pip", branch="main")),
    # Target(repo=Repository(owner="pypa", name="setuptools", branch="main")),
    # Target(repo=Repository(owner="python", name="mypy", branch="master")),
    # Target(
    #     repo=Repository(
    #         owner="python",
    #         name="typeshed",
    #         branch="main",
    #     ),
    #     check_options=CheckOptions(select="PYI"),
    # ),
    # Target(repo=Repository(owner="python-poetry", name="poetry", branch="master")),
    # Target(repo=Repository(owner="reflex-dev", name="reflex", branch="main")),
    # Target(repo=Repository(owner="rotki", name="rotki", branch="develop")),
    # Target(repo=Repository(owner="scikit-build", name="scikit-build", branch="main")),
    # Target(
    #     repo=Repository(owner="scikit-build", name="scikit-build-core", branch="main")
    # ),
    # Target(repo=Repository(owner="sphinx-doc", name="sphinx", branch="master")),
    # Target(repo=Repository(owner="spruceid", name="siwe-py", branch="main")),
    # Target(repo=Repository(owner="tiangolo", name="fastapi", branch="master")),
    # Target(repo=Repository(owner="yandex", name="ch-backup", branch="main")),
    Target(
        repo=Repository(owner="zulip", name="zulip", branch="main"),
        check_options=CheckOptions(select="ALL"),
    ),
]
