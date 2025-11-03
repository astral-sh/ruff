import { fetchPlayground, savePlayground } from "./api";

interface Workspace {
  files: { [name: string]: string };

  // Name of the current file
  current: string;
}

/**
 * Persist the configuration to a URL.
 */
export async function persist(workspace: Workspace): Promise<void> {
  const id = await savePlayground(workspace);

  await navigator.clipboard.writeText(
    `${window.location.origin}/${encodeURIComponent(id)}`,
  );
}

/**
 * Restore the workspace by fetching the data for the ID specified in the URL
 * or by restoring from local storage.
 */
export async function restore(): Promise<Workspace | null> {
  // URLs stored in the database, like:
  //     https://play.ruff.rs/1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed
  const id = window.location.pathname.slice(1);
  if (id !== "") {
    const playground = await fetchPlayground(id);
    if (playground == null) {
      return null;
    }

    return playground;
  }

  // If no URL is present, restore from local storage.
  return restoreLocal();
}

export function persistLocal(workspace: Workspace) {
  let totalLength = 0;
  for (const fileContent of Object.values(workspace.files)) {
    totalLength += fileContent.length;

    // Don't persist large files to local storage because they can exceed the local storage quota
    // The number here is picked rarely arbitrarily. Also note, JS uses UTF 16:
    // that means the limit here is strings larger than 1MB (because UTf 16 uses 2 bytes per character)
    if (totalLength > 500_000) {
      return;
    }
  }

  localStorage.setItem("workspace", JSON.stringify(workspace));
}

function restoreLocal(): Workspace | null {
  const workspace = localStorage.getItem("workspace");

  if (workspace == null) {
    return null;
  } else {
    return JSON.parse(workspace);
  }
}
