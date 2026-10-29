import logging
import subprocess
import sys
from collections.abc import Sequence
from pathlib import Path
from typing import Final, Literal, NamedTuple

logger = logging.getLogger(__name__)


class Project(NamedTuple):
    name: str
    """The name of the project to benchmark."""

    repository: str
    """The git repository to clone."""

    revision: str

    install_arguments: list[str]
    """Arguments passed to `uv pip install`.

    Dependencies are pinned using a `--exclude-newer` flag when installing them
    into the virtual environment; see the `Venv.install()` method for details.
    """

    python_version: Literal["3.13", "3.14", "3.12", "3.11", "3.10", "3.9", "3.8"]

    skip: str | None = None
    """The project is skipped from benchmarking if not `None`."""

    include: Sequence[str] = ()
    """The directories and files to check. If empty, checks the current directory"""

    exclude: Sequence[str] = ()
    """The directories and files to exclude from checks."""

    edit: IncrementalEdit | None = None

    def clone(self, checkout_dir: Path) -> None:
        # Skip cloning if the project has already been cloned (the script doesn't yet support updating)
        if (checkout_dir / ".git").exists():
            return

        logger.debug(f"Cloning {self.repository} to {checkout_dir}")

        try:
            # git doesn't support cloning a specific revision.
            # This is the closest that I found to a "shallow clone with a specific revision"
            subprocess.run(
                [
                    "git",
                    "init",
                    "--quiet",
                ],
                env={"GIT_TERMINAL_PROMPT": "0"},
                cwd=checkout_dir,
                check=True,
                capture_output=True,
                text=True,
            )

            subprocess.run(
                ["git", "remote", "add", "origin", str(self.repository), "--no-fetch"],
                env={"GIT_TERMINAL_PROMPT": "0"},
                cwd=checkout_dir,
                check=True,
                capture_output=True,
                text=True,
            )

            subprocess.run(
                [
                    "git",
                    "fetch",
                    "origin",
                    self.revision,
                    "--quiet",
                    "--depth",
                    "1",
                    "--no-tags",
                ],
                check=True,
                cwd=checkout_dir,
                capture_output=True,
                text=True,
            )

            subprocess.run(
                ["git", "reset", "--hard", "FETCH_HEAD", "--quiet"],
                check=True,
                cwd=checkout_dir,
                capture_output=True,
                text=True,
            )

        except subprocess.CalledProcessError as e:
            raise RuntimeError(f"Failed to clone {self.name}:\n\n{e.stderr}") from e

        logger.info(f"Cloned {self.name} to {checkout_dir}.")


class IncrementalEdit(NamedTuple):
    """Description of an edit to measure incremental performance"""

    edited_file: str
    """The file to which to apply the edit."""

    affected_files: list[str]
    """Files other than the main file that's affected by the edit."""

    replace_text: str
    """The original code snippet to find and replace in the main file."""

    replacement: str
    """The new code snippet to insert in place of `replace_text`"""

    def apply_to(self, text: str) -> str | None:
        """Applies the edit to the given text.

        Returns:
            The modified text or None if the edit couldn't be applied"""

        if self.replace_text not in text:
            return None

        return text.replace(self.replace_text, self.replacement, 1)


# Selection of projects taken from
# [mypy-primer](https://github.com/hauntsaninja/mypy_primer/blob/0ea6cc614b3e91084059b9a3acc58f94c066a211/mypy_primer/projects.py#L71).
# May require frequent updating, especially the dependencies list
ALL: Final = [
    Project(
        name="black",
        repository="https://github.com/psf/black",
        revision="55996d1fb8342725c8a19b6023cd9439c3c1d6f7",
        python_version="3.11",
        include=["src"],
        install_arguments=[
            "-r",
            "pyproject.toml",
            # All extras except jupyter because installing the jupyter optional results in a mypy typing error.
            "--extra",
            "colorama",
            # uvloop is not supported on Windows
            *(["--extra", "uvloop"] if sys.platform != "win32" else []),
            "--extra",
            "d",
        ],
        edit=IncrementalEdit(
            edited_file="src/black/nodes.py",
            replace_text="LN = Union[Leaf, Node]",
            replacement="LN = Union[Leaf, Node, int]",
            affected_files=["src/black/linegen.py"],
        ),
    ),
    Project(
        name="discord.py",
        repository="https://github.com/Rapptz/discord.py.git",
        revision="65232c38702be5844cf2ce865a4777eb1928b5d0",
        python_version="3.11",
        include=["discord"],
        install_arguments=[
            "-r",
            "pyproject.toml",
            "typing_extensions>=4.3,<5",
        ],
        edit=IncrementalEdit(
            edited_file="discord/abc.py",
            replace_text="id: int",
            replacement="id: str",
            affected_files=["discord/channel.py"],
        ),
    ),
    # Fairly chunky project, requires the pydantic mypy plugin.
    Project(
        name="homeassistant",
        repository="https://github.com/home-assistant/core.git",
        revision="c19ae285ede74cd64c3045a3a4aa43290c224f1b",
        python_version="3.14",
        include=["homeassistant"],
        skip="Missing dependencies on Windows" if sys.platform == "win32" else None,
        install_arguments=[
            "-r",
            "requirements_all.txt",
            "-r",
            "requirements_test.txt",
            "--excludes",
            str(Path(__file__).with_name("homeassistant-excludes.txt")),
        ],
        edit=IncrementalEdit(
            edited_file="homeassistant/core.py",
            affected_files=["homeassistant/helpers/event.py"],
            replace_text="type CALLBACK_TYPE = Callable[[], None]",
            replacement="type CALLBACK_TYPE = Callable[[str], None]",
        ),
    ),
    Project(
        name="isort",
        repository="https://github.com/pycqa/isort",
        revision="a514c71ed86c2216d0040c7d60e302ed76a4f7ab",
        python_version="3.11",
        include=["isort"],
        install_arguments=["types-colorama", "colorama"],
        edit=IncrementalEdit(
            edited_file="isort/settings.py",
            replace_text="def is_skipped(self, file_path: Path) -> bool:",
            replacement="def is_skipped(self, file_path: str) -> bool:",
            affected_files=["isort/files.py"],
        ),
    ),
    Project(
        name="jinja",
        repository="https://github.com/pallets/jinja",
        revision="5ef70112a1ff19c05324ff889dd30405b1002044",
        python_version="3.11",
        include=["src"],
        install_arguments=["-r", "pyproject.toml"],
        edit=IncrementalEdit(
            edited_file="src/jinja2/nodes.py",
            replace_text="""def iter_child_nodes(
        self,
        exclude: t.Container[str] | None = None,
        only: t.Container[str] | None = None,
    ) -> t.Iterator["Node"]""",
            replacement="""def iter_child_nodes(
        self,
        exclude: t.Container[str] | None = None,
        only: t.Container[str] | None = None,
    ) -> t.Iterator[str]""",
            affected_files=[
                "src/jinja2/compiler.py",
                "src/jinja2/idtracking.py",
                "src/jinja2/visitor.py",
            ],
        ),
    ),
    Project(
        name="pandas",
        repository="https://github.com/pandas-dev/pandas",
        revision="a0225b65720018f4654390bb16d254daa6ee5e54",
        include=["pandas"],
        exclude=["pandas/tests"],
        python_version="3.11",
        install_arguments=[
            "-r",
            "requirements-dev.txt",
        ],
        edit=IncrementalEdit(
            edited_file="pandas/_typing.py",
            replace_text='Axis: TypeAlias = AxisInt | Literal["index", "columns", "rows"]',
            replacement='Axis: TypeAlias = Literal["index", "columns", "rows"]',
            affected_files=["pandas/core/frame.py"],
        ),
    ),
    Project(
        name="pandas-stubs",
        repository="https://github.com/pandas-dev/pandas-stubs",
        revision="8dd6d91b1c05b26e30f7ddbe3dc3d4042a92bdd6",
        include=["pandas-stubs"],
        # See https://github.com/pandas-dev/pandas-stubs/blob/8dd6d91b1c05b26e30f7ddbe3dc3d4042a92bdd6/pyproject.toml#L8-L72
        python_version="3.11",
        install_arguments=[
            "-r",
            "pyproject.toml",
            "--group",
            "dev",
        ],
        edit=None,  # Tricky in a stubs only project as there are no actual method calls.
    ),
    Project(
        name="prefect",
        repository="https://github.com/PrefectHQ/prefect.git",
        revision="a9452ac21852755311913cb27887b083b5007c03",
        # See https://github.com/PrefectHQ/prefect/blob/a9452ac21852755311913cb27887b083b5007c03/.pre-commit-config.yaml#L25-L39
        include=[
            "src/prefect/server/models",
            "src/prefect/concurrency",
            "src/prefect/events",
            "src/prefect/input",
        ],
        python_version="3.11",
        install_arguments=[
            "-r",
            "pyproject.toml",
            "--group",
            "dev",
        ],
        edit=IncrementalEdit(
            edited_file="src/prefect/server/models/events.py",
            replace_text="""async def deployment_status_event(
    session: AsyncSession,
    deployment_id: UUID,
    status: DeploymentStatus,
    occurred: DateTime,
) -> Event:""",
            replacement="""async def deployment_status_event(
    session: AsyncSession,
    deployment_id: UUID,
    status: DeploymentStatus,
    occurred: DateTime,
) -> int:""",
            affected_files=["src/prefect/server/models/deployments.py"],
        ),
    ),
    Project(
        name="pytorch",
        repository="https://github.com/pytorch/pytorch.git",
        revision="ab92945cea42e47ae398aead69af0872d340ea0f",
        include=["torch", "caffe2"],
        # Based on https://github.com/pytorch/pytorch/blob/ab92945cea42e47ae398aead69af0872d340ea0f/pyrefly.toml#L23-L47
        exclude=[
            "torch/_inductor/codegen/triton.py",
            "tools/linter/adapters/test_device_bias_linter.py",
            "tools/code_analyzer/gen_operators_yaml.py",
            "torch/_inductor/runtime/triton_heuristics.py",
            "torch/_inductor/runtime/triton_helpers.py",
            "torch/_inductor/runtime/halide_helpers.py",
            "torch/utils/tensorboard/summary.py",
            "torch/distributed/flight_recorder/components/types.py",
            "torch/linalg/__init__.py",
            "torch/package/importer.py",
            "torch/package/_package_pickler.py",
            "torch/jit/annotations.py",
            "torch/utils/data/datapipes/_typing.py",
            "torch/nn/functional.py",
            "torch/_export/utils.py",
            "torch/fx/experimental/unification/multipledispatch/__init__.py",
            "torch/nn/modules/__init__.py",
            "torch/nn/modules/rnn.py",
            "torch/_inductor/codecache.py",
            "torch/distributed/elastic/metrics/__init__.py",
            "torch/_inductor/fx_passes/bucketing.py",
            "torch/onnx/_internal/exporter/_torchlib/ops/nn.py",
            "torch/include/**",
            "torch/csrc/**",
            "torch/distributed/elastic/agent/server/api.py",
            "torch/testing/_internal/**",
            "torch/distributed/fsdp/fully_sharded_data_parallel.py",
            "torch/ao/quantization/pt2e/_affine_quantization.py",
            "torch/nn/modules/pooling.py",
            "torch/nn/parallel/_functions.py",
            "torch/_appdirs.py",
            "torch/multiprocessing/pool.py",
            "torch/overrides.py",
            "*/__pycache__/**",
            "*/.*",
        ],
        # Based on the upstream lint environment, keeping only packages needed
        # to resolve imports from torch/ and caffe2/. Checker versions are managed
        # separately by the benchmark.
        # https://github.com/pytorch/pytorch/blob/ab92945cea42e47ae398aead69af0872d340ea0f/tools/linter/adapters/pyrefly_linter.py#L1-L28
        install_arguments=[
            "numpy",
            "expecttest",
            "sympy",
            "spmd-types",
            "types-requests",
            "types-pyyaml",
            "types-tabulate",
            "types-protobuf",
            "types-setuptools",
            "types-jinja2",
            "types-colorama",
            "filelock",
            "junitparser",
            "rich",
            "optree",
            "types-openpyxl",
            "types-python-dateutil",
        ],
        python_version="3.11",
        edit=IncrementalEdit(
            edited_file="torch/nn/__init__.py",
            replace_text="""from torch.nn.parameter import (  # usort: skip
    Buffer as Buffer,
    Parameter as Parameter,
    UninitializedBuffer as UninitializedBuffer,
    UninitializedParameter as UninitializedParameter,
)""",
            replacement="""from torch.nn.parameter import (  # usort: skip
    Buffer as Buffer,
    UninitializedBuffer as UninitializedBuffer,
    UninitializedParameter as UninitializedParameter,
)""",
            affected_files=[
                "torch/distributed/pipelining/_backward.py",
            ],
        ),
    ),
    Project(
        name="httpx",
        repository="https://github.com/encode/httpx",
        revision="b5addb64f0161ff6bfe94c124ef76f6a1fba5254",
        python_version="3.11",
        include=["httpx"],
        install_arguments=["-r", "pyproject.toml", "--all-extras"],
        edit=IncrementalEdit(
            edited_file="httpx/_urls.py",
            replace_text="def raw_path(self) -> bytes:",
            replacement="def raw_path(self) -> str:",
            affected_files=["httpx/_client.py"],
        ),
    ),
    Project(
        name="numpy",
        repository="https://github.com/numpy/numpy",
        revision="f9f1772ce010001208475f751d19253469788410",
        python_version="3.12",
        include=["numpy"],
        exclude=["numpy/tests/**", "numpy/**/tests/**"],
        # Check the Python implementation and array stubs without building C extensions.
        install_arguments=["typing-extensions", "packaging"],
        edit=IncrementalEdit(
            edited_file="numpy/_core/multiarray.pyi",
            replace_text="def normalize_axis_index(axis: int, ndim: int, msg_prefix: str | None = None) -> int: ...",
            replacement="def normalize_axis_index(axis: int, ndim: int, msg_prefix: str | None = None) -> str: ...",
            affected_files=["numpy/_core/numeric.py"],
        ),
    ),
    Project(
        name="fastapi",
        repository="https://github.com/fastapi/fastapi",
        revision="7a0a3f0b286c6c865cf4398ea0e957a87b8bffa4",
        python_version="3.11",
        include=["fastapi"],
        install_arguments=["-r", "pyproject.toml", "--extra", "standard"],
        edit=IncrementalEdit(
            edited_file="fastapi/utils.py",
            replace_text='def generate_unique_id(route: "APIRoute") -> str:',
            replacement='def generate_unique_id(route: "APIRoute") -> int:',
            affected_files=["fastapi/routing.py"],
        ),
    ),
    Project(
        name="sqlalchemy",
        repository="https://github.com/sqlalchemy/sqlalchemy",
        revision="10f4d1e2d706a7722ca78e79ce05b00333a7aa5b",
        python_version="3.11",
        include=["lib/sqlalchemy"],
        install_arguments=[
            "-r",
            "pyproject.toml",
            "--extra",
            "asyncio",
            "types-greenlet",
            "pytest",
        ],
        edit=IncrementalEdit(
            edited_file="lib/sqlalchemy/util/_collections.py",
            replace_text="def to_list(x: Any, default: Optional[List[Any]] = None) -> List[Any]:",
            replacement="def to_list(x: Any, default: Optional[List[Any]] = None) -> int:",
            affected_files=["lib/sqlalchemy/orm/mapper.py"],
        ),
    ),
]
