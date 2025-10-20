import lzstring from "lz-string";
import { fetchPlayground, savePlayground } from "./api";

export type Settings = { [K: string]: any };

/**
 * Stringify a settings object to JSON.
 */
export function stringify(settings: Settings): string {
  return JSON.stringify(
    settings,
    (k, v) => {
      if (v instanceof Map) {
        return Object.fromEntries(v.entries());
      } else {
        return v;
      }
    },
    2,
  );
}

/**
 * Persist the configuration to a URL.
 */
export async function persist(
  settingsSource: string,
  pythonSource: string,
): Promise<void> {
  const id = await savePlayground({ settingsSource, pythonSource });
  await navigator.clipboard.writeText(`${window.location.origin}/${id}`);
}

/**
 * Restore the configuration from a URL.
 */
export async function restore(): Promise<[string, string] | null> {
  // Legacy URLs, stored as encoded strings in the hash, like:
  //     https://play.ruff.rs/#eyJzZXR0aW5nc1NvdXJjZ...
  const hash = window.location.hash.slice(1);
  if (hash) {
    const value = lzstring.decompressFromEncodedURIComponent(
      window.location.hash.slice(1),
    )!;
    const [settingsSource, pythonSource] = value.split("$$$");
    return [settingsSource.replaceAll("$$$$$$", "$$$"), pythonSource];
  }

  // URLs stored in the database, like:
  //     https://play.ruff.rs/1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed
  const id = window.location.pathname.slice(1);
  if (id) {
    const playground = await fetchPlayground(id);
    if (playground == null) {
      return null;
    }
    const { settingsSource, pythonSource } = playground;
    return [settingsSource, pythonSource];
  }

  // If no URL is present, restore from local storage.
  return restoreLocal();
}

function restoreLocal(): [string, string] | null {
  const source = localStorage.getItem("source");

  if (source == null) {
    return null;
  } else {
    return JSON.parse(source);
  }
}

export function persistLocal({
  settingsSource,
  pythonSource,
}: {
  settingsSource: string;
  pythonSource: string;
}) {
  const totalLength = settingsSource.length + pythonSource.length;

  // Don't persist large files to local storage because they can exceed the local storage quota
  // The number here is picked rarely arbitrarily. Also note, JS uses UTF 16:
  // that means the limit here is strings larger than 1MB (because UTf 16 uses 2 bytes per character)
  if (totalLength > 500_000) {
    return;
  }

  localStorage.setItem(
    "source",
    JSON.stringify([settingsSource, pythonSource]),
  );
}
