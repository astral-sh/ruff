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
    `${window.location.origin}${window.location.pathname}?id=${id}`,
  );
}

/**
 * Restore the workspace by fetching the data for the ID specified in the URL
 * or by restoring from local storage.
 */
export async function restore(): Promise<Workspace | null> {
  const params = new URLSearchParams(window.location.search);

  const id = params.get("id");

  if (id != null) {
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
