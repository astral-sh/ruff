# isort: off
# pylint: disable=import-error

from dagster import (
    load_assets_from_package_module,
)

# start_example

import my_package.cereal as cereal

cereal_assets = load_assets_from_package_module(
    cereal,
    group_name="cereal_assets",
)

# end_example

