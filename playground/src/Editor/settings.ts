import lzstring from "lz-string";
import { v4 as uuidv4 } from "uuid";
import { get, set } from "./db";

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
  const id = uuidv4();
  await set(id, { settingsSource, pythonSource });
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
    const response = await get<{
      settingsSource: string;
      pythonSource: string;
    }>(id);
    if (response == null) {
      return null;
    }
    const { settingsSource, pythonSource } = response;
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
  localStorage.setItem(
    "source",
    JSON.stringify([settingsSource, pythonSource]),
  );
}
