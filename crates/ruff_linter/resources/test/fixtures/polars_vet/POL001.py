import polars as pl

pl.read_csv("data.csv").lazy()
pl.read_parquet("data.parquet").lazy()
pl.read_ndjson("data.ndjson").lazy()
pl.read_ipc("data.ipc").lazy()

# No equivalent scan function.
pl.read_excel("data.xlsx").lazy()

# Already lazy-first API.
pl.scan_csv("data.csv")

# Not a lazy chain.
pl.read_csv("data.csv")

# Still flag even if lazy receives arguments.
pl.read_csv("data.csv").lazy(streaming=True)
