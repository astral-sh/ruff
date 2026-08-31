<!-- Begin section: Overview -->

# Ruff

[![Ruff](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/ruff/main/assets/badge/v2.json)](https://github.com/astral-sh/ruff)
[![image](https://img.shields.io/pypi/v/ruff.svg)](https://pypi.python.org/pypi/ruff)
[![image](https://img.shields.io/pypi/l/ruff.svg)](https://github.com/astral-sh/ruff/blob/main/LICENSE)
[![image](https://img.shields.io/pypi/pyversions/ruff.svg)](https://pypi.python.org/pypi/ruff)
[![Actions status](https://github.com/astral-sh/ruff/workflows/CI/badge.svg)](https://github.com/astral-sh/ruff/actions)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white)](https://discord.com/invite/astral-sh)

<p align="center">
  <a href="README.md">English</a> · <b>简体中文</b>
</p>

[**官方文档**](https://docs.astral.sh/ruff/) | [**在线演练场 (Playground)**](https://play.ruff.rs/)

基于 Rust 编写的极速 Python 代码分析器 (Linter) 与代码格式化工具 (Formatter)。

<p align="center">
  <picture align="center">
    <source media="(prefers-color-scheme: dark)" srcset="https://user-images.githubusercontent.com/1309177/232603514-c95e9b0f-6b31-43de-9a80-9e844173fd6a.svg">
    <source media="(prefers-color-scheme: light)" srcset="https://user-images.githubusercontent.com/1309177/232603516-4fb4892d-585c-4b20-b810-3db9161831e4.svg">
    <img alt="展示基准测试结果的条形图" src="https://user-images.githubusercontent.com/1309177/232603516-4fb4892d-585c-4b20-b810-3db9161831e4.svg">
  </picture>
</p>

<p align="center">
  <i>从零开始对 CPython 代码库进行全量代码静态检查。</i>
</p>

- ⚡️ 比现有 Linter（如 Flake8）和格式化工具（如 Black）**快 10 到 100 倍**
- 🐍 支持通过 `pip` 一键安装
- 🛠️ 原生支持 `pyproject.toml` 配置文件
- 🤝 兼容 Python 3.14 前沿语法
- ⚖️ 完美平替 [Flake8](https://docs.astral.sh/ruff/faq/#how-does-ruffs-linter-compare-to-flake8)、[isort](https://docs.astral.sh/ruff/faq/#how-does-ruffs-import-sorting-compare-to-isort) 与 [Black](https://docs.astral.sh/ruff/faq/#how-does-ruffs-formatter-compare-to-black)
- 📦 内置高效缓存，自动跳过未修改文件
- 🔧 支持 `--fix` 自动修复错误（例如自动清除无用 import）
- 📏 内置 **超过 900 条检查规则**，原生重写了流行 Flake8 插件（如 flake8-bugbear）
- ⌨️ 提供官方第一方 [编辑器集成](https://docs.astral.sh/ruff/editors)（支持 [VS Code](https://github.com/astral-sh/ruff-vscode) 等各类主流 IDE）
- 🌎 对 Monorepo 单体多包仓库极度友好，支持[层级与级联继承配置](https://docs.astral.sh/ruff/configuration/#config-file-discovery)

Ruff 的目标是在提供统一整洁 CLI 接口的同时，带来比同类工具快几个数量级的极致性能。

Ruff 可一站式替代 [Flake8](https://pypi.org/project/flake8/)（及数十个插件）、[Black](https://github.com/psf/black)、[isort](https://pypi.org/project/isort/)、[pydocstyle](https://pypi.org/project/pydocstyle/)、[pyupgrade](https://pypi.org/project/pyupgrade/)、[autoflake](https://pypi.org/project/autoflake/) 等工具，且运行速度比任何单一工具快数十至数百倍。

Ruff 处于极其活跃的迭代状态，已被各大顶级开源项目广泛采用：
- [Apache Airflow](https://github.com/apache/airflow)
- [Apache Superset](https://github.com/apache/superset)
- [FastAPI](https://github.com/tiangolo/fastapi)
- [Hugging Face](https://github.com/huggingface/transformers)
- [Pandas](https://github.com/pandas-dev/pandas)
- [SciPy](https://github.com/scipy/scipy)

...以及 [众多企业与开源项目](#哪些项目正在使用-ruff)。

Ruff 由 [Astral](https://astral.sh) 团队开发并维护，他们也是 [uv](https://github.com/astral-sh/uv) 与 [ty](https://github.com/astral-sh/ty) 的开发者。

阅读 [发布博客](https://astral.sh/blog/announcing-astral-the-company-behind-ruff) 或最初的 [技术背景宣言](https://notes.crmarsh.com/python-tooling-could-be-much-much-faster)。

## 业界评价

[**Sebastián Ramírez**](https://twitter.com/tiangolo/status/1591912354882764802)（[FastAPI](https://github.com/tiangolo/fastapi) 创作者）：
> “Ruff 实在太快了，以至于有时候我会在代码里故意写个 bug，只是为了确认它真的在运行并检查了代码。”

[**Nick Schrock**](https://twitter.com/schrockn/status/1612615862904827904)（[GraphQL](https://graphql.org/) 联合创作者、[Elementl](https://www.elementl.com/) 创始人）：
> “为什么 Ruff 是颠覆性的？主要是因为它几乎快了 1000 倍。千真万确，没有夸张。在我们最大的模块（dagster 本身，25 万行代码）中，pylint 在我的 M1 电脑上开启 4 核并行需要大约 2.5 分钟；而对我们的全部代码库运行 ruff 只需要 0.4 秒。”

[**Bryan Van de Ven**](https://github.com/bokeh/bokeh/pull/12605)（[Bokeh](https://github.com/bokeh/bokeh/) 联合创作者、[Conda](https://docs.conda.io/en/latest/) 原作者）：
> “在我的电脑上，Ruff 比 flake8 快约 150-200 倍，扫描整个仓库只需要 ~0.2 秒而不是 ~20 秒。这对本地开发体验是巨大的质的飞跃。它快到我直接把它加入了实际的 commit hook，太棒了。”

[**Timothy Crosley**](https://twitter.com/timothycrosley/status/1606420868514877440)（[isort](https://github.com/PyCQA/isort) 创作者）：
> “刚刚把我的第一个项目切换到了 Ruff。目前唯一的‘缺点’：它快到让我难以置信它真的工作了，直到我故意引入了一些错误。”

[**Tim Abbott**](https://github.com/zulip/zulip/pull/23431#issuecomment-1302557034)（[Zulip](https://github.com/zulip/zulip) 核心负责人）：
> “这速度快得简直离谱…… `ruff` 太神了。”

<!-- End section: Overview -->

## 目录

更多详情请参阅[官方文档](https://docs.astral.sh/ruff/)。

1. [快速开始](#快速开始)
1. [项目配置](#项目配置)
1. [检查规则](#检查规则)
1. [参与贡献](#参与贡献)
1. [技术支持](#技术支持)
1. [致谢](#致谢)
1. [哪些项目正在使用 Ruff？](#哪些项目正在使用-ruff)
1. [开源许可证](#开源许可证)

## 快速开始

### 安装指南

Ruff 在 PyPI 上的包名为 [`ruff`](https://pypi.org/project/ruff/)。

可通过 [`uvx`](https://docs.astral.sh/uv/) 直接即开即用调用 Ruff：

```shell
uvx ruff@0.16.5 check   # 检查当前目录中的所有文件 (Lint)
uvx ruff@0.16.5 format  # 格式化当前目录中的所有文件 (Format)
```

或使用 `uv`（推荐）、`pip` 或 `pipx` 进行安装：

```shell
# 使用 uv (推荐)
uv tool install ruff@latest  # 全局安装 Ruff CLI
uv add --dev ruff            # 或将 Ruff 作为开发依赖添加到当前项目

# 使用 pip
pip install ruff

# 使用 pipx
pipx install ruff
```

从 `0.5.0` 版本开始，支持通过独立脚本安装：

```shell
# macOS 与 Linux
curl -LsSf https://astral.sh/ruff/install.sh | sh

# Windows (PowerShell)
powershell -c "irm https://astral.sh/ruff/install.ps1 | iex"
```

也可以通过 [Homebrew](https://formulae.brew.sh/formula/ruff)（`brew install ruff`）、[Conda](https://anaconda.org/conda-forge/ruff) 等方式安装。

### 基本用法

作为 **Linter** 代码检查器运行：

```shell
ruff check                          # 检查当前目录及所有子目录下的文件
ruff check path/to/code/            # 检查指定路径目录下的文件
ruff check path/to/code/*.py        # 检查指定路径下的所有 .py 文件
ruff check path/to/code/to/file.py  # 检查单个指定文件
ruff check --fix                    # 自动修复所有可安全自动修复的规则错误
```

作为 **Formatter** 代码格式化工具运行：

```shell
ruff format                          # 格式化当前目录及所有子目录下的所有文件
ruff format path/to/code/            # 格式化指定目录
ruff format --check                  # 仅检查格式，不直接修改文件
```

在 [pre-commit](https://pre-commit.com/) 中通过 [`ruff-pre-commit`](https://github.com/astral-sh/ruff-pre-commit) 使用：

```yaml
- repo: https://github.com/astral-sh/ruff-pre-commit
  rev: v0.16.5
  hooks:
    # 运行代码检查与自动修复
    - id: ruff-check
      args: [ --fix ]
    # 运行代码格式化
    - id: ruff-format
```

在 GitHub Actions 中通过 [`ruff-action`](https://github.com/astral-sh/ruff-action) 使用：

```yaml
name: Ruff
on: [ push, pull_request ]
jobs:
  ruff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/ruff-action@v3
```

### 项目配置

Ruff 支持通过 `pyproject.toml`、`ruff.toml` 或 `.ruff.toml` 进行灵活配置（参阅[配置选项完整指南](https://docs.astral.sh/ruff/configuration/)与[设置项列表](https://docs.astral.sh/ruff/settings/)）。

默认规则可查阅 [默认规则列表](https://docs.astral.sh/ruff/default-rules/)。

默认未指定时的行为等价于以下 `ruff.toml` 配置：

```toml
# 忽略常见的非代码或构建生成目录
exclude = [
    ".bzr",
    ".direnv",
    ".eggs",
    ".git",
    ".git-rewrite",
    ".hg",
    ".ipynb_checkpoints",
    ".mypy_cache",
    ".nox",
    ".pants.d",
    ".pyenv",
    ".pytest_cache",
    ".pytype",
    ".ruff_cache",
    ".svn",
    ".tox",
    ".venv",
    ".vscode",
    "__pypackages__",
    "_build",
    "buck-out",
    "build",
    "dist",
    "node_modules",
    "site-packages",
    "venv",
]

# 与 Black 保持一致的单行最大字符数与缩进
line-length = 88
indent-width = 4

# 目标 Python 版本
target-version = "py310"

[lint]
# select = [...]  # 默认启用的规则类别
ignore = []

# 允许对所有启用的规则进行自动修复 (当传入 --fix 时)
fixable = ["ALL"]
unfixable = []

# 允许下划线前缀的未使用变量
dummy-variable-rgx = "^(_+|(_+[a-zA-Z0-9_]*[a-zA-Z0-9]+?))$"

[format]
# 与 Black 一致，字符串默认使用双引号
quote-style = "double"

# 使用空格缩进而非 Tab
indent-style = "space"

# 尊重魔法末尾逗号
skip-magic-trailing-comma = false

# 自动检测换行符
line-ending = "auto"
```

在 `pyproject.toml` 中配置时，请在小节前加上 `tool.ruff` 前缀，例如 `[lint]` 写作 `[tool.ruff.lint]`。

## 检查规则

**Ruff 支持超过 900 条代码检查规则**，涵盖了 Flake8、isort、pyupgrade 等流行工具的规则体系，并在 Rust 中进行了第一方原生重写实现。

默认情况下，Ruff 开启了 `F` (Pyflakes)、`E` (pycodestyle 错误)、`B` (flake8-bugbear)、`UP` (pyupgrade) 与 `RUF` (Ruff 专属规则) 等类别的核心规则，自动过滤了会与格式化工具冲突的纯代码排版规则。

查看全部支持的规则及代码代号请访问 [完整规则清单](https://docs.astral.sh/ruff/rules/)。

## 参与贡献

热烈欢迎向 Ruff 提交贡献！请查阅[贡献指南 (CONTRIBUTING)](https://docs.astral.sh/ruff/contributing/) 开启贡献。

欢迎加入 [Discord 开发者频道](https://discord.com/invite/astral-sh)。

## 致谢

- Ruff 的 Linter 借鉴了 Python 生态中众多优秀工具的 API 和实现细节，特别是 [Flake8](https://github.com/PyCQA/flake8)、[Pyflakes](https://github.com/PyCQA/pyflakes)、[pycodestyle](https://github.com/PyCQA/pycodestyle)、[pydocstyle](https://github.com/PyCQA/pydocstyle)、[pyupgrade](https://github.com/asottile/pyupgrade) 和 [isort](https://github.com/PyCQA/isort)。
- Ruff 的 Formatter 格式化引擎基于 Rome 项目的 [`rome_formatter`](https://github.com/rome/tools/tree/main/crates/rome_formatter) 分支构建，并吸收了 [Prettier](https://github.com/prettier/prettier) 与 [Black](https://github.com/psf/black) 的设计。
- 模块导入解析器基于 [Pyright](https://github.com/microsoft/pyright) 的解析算法。
- 同时受到了 Rust 社区 [Clippy](https://github.com/rust-lang/rust-clippy) 与 JS 社区 [ESLint](https://github.com/eslint/eslint) 的深刻启发。

## 开源许可证

Ruff 基于 [MIT 开源许可证](https://github.com/astral-sh/ruff/blob/main/LICENSE) 发布。

<div align="center">
  <a target="_blank" href="https://astral.sh" style="background:none">
    <img src="https://raw.githubusercontent.com/astral-sh/ruff/main/assets/svg/Astral.svg" alt="Made by Astral">
  </a>
</div>

---

> 💡 **文档维护说明**：本中文文档由社区志愿者（@JasonYeYuhe）翻译维护，最后同步更新于 2026年8月31日。如发现内容与官方英文原版存在差异或新特性滞后，欢迎提交 PR 共同完善！
