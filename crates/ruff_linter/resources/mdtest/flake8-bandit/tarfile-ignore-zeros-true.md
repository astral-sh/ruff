# `tarfile-ignore-zeros-true` (`S203`)

```toml
lint.preview = true
lint.select = ["S203"]
```

## Basic examples

Opening an archive with `ignore_zeros=True` switches `tarfile` into a permissive mode that
skips empty and invalid blocks instead of stopping at them. We report the argument itself, so
the diagnostic points at the flag rather than at the whole call.

```py
import tarfile

tarfile.open("archive.tar", ignore_zeros=True)  # snapshot: tarfile-ignore-zeros-true
```

```snapshot
error[S203]: `tarfile` opened with `ignore_zeros=True`
 --> src/mdtest_snippet.py:3:29
  |
3 | tarfile.open("archive.tar", ignore_zeros=True)  # snapshot: tarfile-ignore-zeros-true
  |                             ^^^^^^^^^^^^^^^^^
```

The module-level `tarfile.open` is an alias for `TarFile.open`; the constructor and the
compression-specific class methods accept the same keyword.

```py
import tarfile
from tarfile import TarFile
from tarfile import open as tar_open

tarfile.TarFile("archive.tar", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
tarfile.TarFile.open("archive.tar", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
TarFile("archive.tar", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
TarFile.open("archive.tar", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
tar_open("archive.tar", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
tarfile.TarFile.taropen("archive.tar", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
tarfile.TarFile.gzopen("archive.tar.gz", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
tarfile.TarFile.bz2open("archive.tar.bz2", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
tarfile.TarFile.xzopen("archive.tar.xz", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]
tarfile.TarFile.zstopen("archive.tar.zst", ignore_zeros=True)  # error: [tarfile-ignore-zeros-true]

with tarfile.open("archive.tar", "r", ignore_zeros=True) as tar:  # error: [tarfile-ignore-zeros-true]
    pass
```

The `TarFile` constructor also accepts `ignore_zeros` positionally, as its seventh parameter.

```py
import tarfile

tarfile.TarFile("archive.tar", "r", None, None, None, None, True)  # error: [tarfile-ignore-zeros-true]
```

## Truthy non-`bool` values

Like `call-with-shell-equals-true` (`S604`), we flag any value that is statically known to be
truthy, and the message says "truthy" instead of `True` so it does not misquote the code.

```py
import tarfile

tarfile.open("archive.tar", ignore_zeros=1)  # snapshot: tarfile-ignore-zeros-true
```

```snapshot
error[S203]: `tarfile` opened with truthy `ignore_zeros`
 --> src/mdtest_snippet.py:3:29
  |
3 | tarfile.open("archive.tar", ignore_zeros=1)  # snapshot: tarfile-ignore-zeros-true
  |                             ^^^^^^^^^^^^^^
```

## No errors

Omitting the flag, or passing one of the values that keep the default behavior, is fine.

```py
import tarfile

tarfile.open("archive.tar")
tarfile.open("archive.tar", ignore_zeros=False)
tarfile.open("archive.tar", ignore_zeros=None)
tarfile.open("archive.tar", ignore_zeros=0)
tarfile.TarFile("archive.tar", "r", None, None, None, None, False)
```

A value that cannot be evaluated statically is not flagged.

```py
import tarfile


def open_archive(name, lenient):
    return tarfile.open(name, ignore_zeros=lenient)
```

Callables that merely share a name with the `tarfile` openers are not flagged, and neither are
`tarfile` functions that do not accept the flag.

```py
import tarfile


class Opener:
    def open(self, name, ignore_zeros=False): ...


Opener().open("archive.tar", ignore_zeros=True)
tarfile.is_tarfile("archive.tar", ignore_zeros=True)
```
