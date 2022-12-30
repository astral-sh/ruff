import lzstring from "lz-string";

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
export function persist(settingsSource: string, pythonSource: string) {
  window.location.hash = lzstring.compressToEncodedURIComponent(
    settingsSource + "$$$" + pythonSource,
  );
}

/**
 * Restore the configuration from a URL.
 */
export function restore(): [string, string] | null {
  const value = lzstring.decompressFromEncodedURIComponent(
    window.location.hash.slice(1),
  );

  if (value) {
    const parts = value.split("$$$");
    const settingsSource = parts[0];
    const pythonSource = parts[1];
    return [settingsSource, pythonSource];
  } else {
    return null;
  }
}
