import logging
import subprocess
import sys
from pathlib import Path
from typing import Final, Literal, NamedTuple


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

    include: list[str] = []
    """The directories and files to check. If empty, checks the current directory"""

    exclude: list[str] = []
    """The directories and files to exclude from checks."""

    edit: IncrementalEdit | None = None

    def clone(self, checkout_dir: Path) -> None:
        # Skip cloning if the project has already been cloned (the script doesn't yet support updating)
        if (checkout_dir / ".git").exists():
            return

        logging.debug(f"Cloning {self.repository} to {checkout_dir}")

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
            raise RuntimeError(f"Failed to clone {self.name}: {e.stderr}")

        logging.info(f"Cloned {self.name} to {checkout_dir}.")


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
        revision="45b4087976b7880db9dabacc992ee142f2d6c7c7",
        python_version="3.10",
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
        revision="9be91cb093402f54a44726c7dc4c04ff3b2c5a63",
        python_version="3.8",
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
    #
    # Pyrefly reports significantely more diagnostics than ty and, unlike ty, has partial pydantic support.
    # Both could be the reason why Pyrefly is slower than ty (it's notable that it's mainly slower because it has a much higher system time)
    Project(
        name="homeassistant",
        repository="https://github.com/home-assistant/core.git",
        revision="7fd440c4a06777bc4cfd90a3c176ded80c87a8fd",
        python_version="3.14",
        include=["homeassistant"],
        skip="Missing dependencies on Windows" if sys.platform == "win32" else None,
        install_arguments=[
            "-r",
            "requirements_test_all.txt",
            "-r",
            "requirements.txt",
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
        revision="ed501f10cb5c1b17aad67358017af18cf533c166",
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
        python_version="3.10",
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
        revision="4d8348341bc4de2f0f90782ecef1b092b9418a19",
        include=["pandas", "typings"],
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
        revision="ad8cae5bc1f0bc87ce22b4d445e0700976c9dfb4",
        include=["pandas-stubs"],
        python_version="3.10",
        # Uses poetry :(
        install_arguments=[
            "types-pytz >=2022.1.1",
            "types-python-dateutil>=2.8.19",
            "numpy >=1.23.5",
            "pyarrow >=10.0.1",
            "matplotlib >=3.10.1",
            "xarray>=22.6.0",
            "SQLAlchemy>=2.0.39",
            "odfpy >=1.4.1",
            "pyxlsb >=1.0.10",
            "jinja2 >=3.1",
            "scipy >=1.9.1",
            "scipy-stubs >=1.15.3.0",
        ],
        edit=None,  # Tricky in a stubs only project as there are no actual method calls.
    ),
    Project(
        name="prefect",
        repository="https://github.com/PrefectHQ/prefect.git",
        revision="a3db33d4f9ee7a665430ae6017c649d057139bd3",
        # See https://github.com/PrefectHQ/prefect/blob/a3db33d4f9ee7a665430ae6017c649d057139bd3/.pre-commit-config.yaml#L33-L39
        include=[
            "src/prefect/server/models",
            "src/prefect/concurrency",
            "src/prefect/events",
            "src/prefect/input",
        ],
        python_version="3.10",
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
        revision="be33b7faf685560bb618561b44b751713a660337",
        include=["torch", "caffe2"],
        # see https://github.com/pytorch/pytorch/blob/c56655268b4ae575ee4c89c312fd93ca2f5b3ba9/pyrefly.toml#L23
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
        # See https://github.com/pytorch/pytorch/blob/be33b7faf685560bb618561b44b751713a660337/.lintrunner.toml#L141
        install_arguments=[
            'numpy==1.26.4 ; python_version >= "3.10" and python_version <= "3.11"',
            'numpy==2.1.0 ; python_version >= "3.12" and python_version <= "3.13"',
            'numpy==2.3.4 ; python_version >= "3.14"',
            "expecttest==0.3.0",
            "pyrefly==0.36.2",
            "sympy==1.13.3",
            "types-requests==2.27.25",
            "types-pyyaml==6.0.2",
            "types-tabulate==0.8.8",
            "types-protobuf==5.29.1.20250403",
            "types-setuptools==79.0.0.20250422",
            "types-jinja2==2.11.9",
            "types-colorama==0.4.6",
            "filelock==3.18.0",
            "junitparser==2.1.1",
            "rich==14.1.0",
            "optree==0.17.0",
            "types-openpyxl==3.1.5.20250919",
            "types-python-dateutil==2.9.0.20251008",
            "mypy==1.16.0",  # pytorch pins mypy,
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
]
