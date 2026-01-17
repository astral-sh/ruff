from __future__ import annotations

from typing import cast

import prefect_azure.credentials


async def execute_bundle(
    credentials_block_name: str,
) -> None:
    if credentials_block_name:
        abs_credentials = cast(
            prefect_azure.credentials.AzureBlobStorageCredentials,
            await prefect_azure.credentials.AzureBlobStorageCredentials.load(
                credentials_block_name,
                _sync=False,
            ),
        )
    else:
        abs_credentials = prefect_azure.credentials.AzureBlobStorageCredentials()
