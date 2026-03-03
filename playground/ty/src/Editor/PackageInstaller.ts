import { unzipSync } from "fflate";
import type { Workspace } from "ty_wasm";

export interface InstalledPackageInfo {
  name: string;
  version: string;
}

export interface InstallationStatus {
  state: "idle" | "installing" | "success" | "error";
  message: string;
  /** 0–100 progress percentage, or null when indeterminate (e.g. resolving) */
  progress: number | null;
  installedPackages: InstalledPackageInfo[];
  writtenFiles: string[];
}

export const IDLE_STATUS: InstallationStatus = {
  state: "idle",
  message: "",
  progress: null,
  installedPackages: [],
  writtenFiles: [],
};

export const PACKAGES_ROOT = "/packages";

/**
 * Module-level store of extracted package files for use by Pyodide runtime.
 * Updated each time packages are installed.
 */
let extractedPackageFiles: Array<{ path: string; contents: string }> = [];

export function getExtractedPackageFiles(): Array<{
  path: string;
  contents: string;
}> {
  return extractedPackageFiles;
}

/**
 * Cheap guard to prevent runaway BFS over the dependency graph.
 * Without this, transitive deps can explode (flask alone pulls 7 direct deps,
 * each with their own trees) and cyclic or deep chains can stall the installer.
 */
const MAX_DEPENDENCY_DEPTH = 3;
const PYPI_API = "https://pypi.org/pypi";

interface PyPIInfo {
  info: {
    name: string;
    version: string;
    requires_dist: string[] | null;
  };
  urls: Array<{
    filename: string;
    url: string;
    packagetype: string;
  }>;
}

interface ResolvedPackage {
  name: string;
  version: string;
  wheelUrl: string;
  requiresDist: string[] | null;
}

/**
 * Installs packages from PyPI into the workspace's memory file system.
 *
 * Flow:
 * 1. Resolve all dependencies (BFS, with depth limit)
 * 2. Download and extract pure-Python wheels
 * 3. Write .py/.pyi files to /packages/ in the MemoryFS
 */
export async function installPackages(
  workspace: Workspace,
  packages: string[],
  previousFiles: string[],
  onProgress: (status: InstallationStatus) => void,
  signal?: AbortSignal,
): Promise<InstallationStatus> {
  if (packages.length === 0) {
    // Clean up previous files if dependencies were removed
    if (previousFiles.length > 0) {
      workspace.removePackageFiles(previousFiles);
    }
    extractedPackageFiles = [];
    return IDLE_STATUS;
  }

  onProgress({
    state: "installing",
    message: "Resolving dependencies...",
    progress: null,
    installedPackages: [],
    writtenFiles: [],
  });

  try {
    // Resolve all transitive dependencies
    const resolved = await resolveAllDeps(packages, signal);

    if (signal?.aborted) {
      return IDLE_STATUS;
    }

    // Remove previously written files
    if (previousFiles.length > 0) {
      workspace.removePackageFiles(previousFiles);
    }

    const allWrittenFiles: string[] = [];
    const allExtractedFiles: Array<{ path: string; contents: string }> = [];
    const installed: InstalledPackageInfo[] = [];
    const total = resolved.length;

    for (let i = 0; i < resolved.length; i++) {
      const pkg = resolved[i];

      if (signal?.aborted) {
        return IDLE_STATUS;
      }

      const pct = Math.round((i / total) * 100);

      onProgress({
        state: "installing",
        message: `Installing ${pkg.name} ${pkg.version}...`,
        progress: pct,
        installedPackages: installed,
        writtenFiles: allWrittenFiles,
      });

      const files = await downloadAndExtractWheel(pkg.wheelUrl, signal);

      if (signal?.aborted) {
        return IDLE_STATUS;
      }

      const packageFiles = files.map(({ path, contents }) => ({
        path: `${PACKAGES_ROOT}/${path}`,
        contents,
      }));

      if (packageFiles.length > 0) {
        workspace.writePackageFiles(packageFiles);
        allWrittenFiles.push(...packageFiles.map((f) => f.path));
        allExtractedFiles.push(...packageFiles);
      }

      installed.push({
        name: pkg.name,
        version: pkg.version,
      });
    }

    // Store extracted files for Pyodide runtime
    extractedPackageFiles = allExtractedFiles;

    const result: InstallationStatus = {
      state: "success",
      message: `Installed ${installed.length} package(s)`,
      progress: 100,
      installedPackages: installed,
      writtenFiles: allWrittenFiles,
    };

    onProgress(result);
    return result;
  } catch (error) {
    const result: InstallationStatus = {
      state: "error",
      message: formatError(error),
      progress: null,
      installedPackages: [],
      writtenFiles: previousFiles,
    };
    onProgress(result);
    return result;
  }
}

/**
 * Parse a dependency specifier into name and optional pinned version.
 * Supports: "requests", "requests==2.28.0", "requests>=2.0" (version ignored).
 */
function parseDependencySpec(spec: string): { name: string; version: string | null } {
  const match = spec.match(/^([A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?)(?:==(.+))?$/);
  if (match) {
    return { name: match[1], version: match[3] ?? null };
  }
  // Strip other specifiers (>=, ~=, !=, etc.) — just use latest
  const nameOnly = spec.match(/^([A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?)/);
  return { name: nameOnly ? nameOnly[1] : spec, version: null };
}

async function fetchPackageInfo(
  name: string,
  version: string | null,
  signal?: AbortSignal,
): Promise<PyPIInfo> {
  const versionSuffix = version != null ? `/${version}` : "";
  const response = await fetch(
    `${PYPI_API}/${encodeURIComponent(name)}${versionSuffix}/json`,
    { signal },
  );
  if (!response.ok) {
    if (response.status === 404) {
      const detail = version != null ? `'${name}==${version}'` : `'${name}'`;
      throw new Error(`Package ${detail} not found on PyPI`);
    }
    throw new Error(
      `Failed to fetch package info for '${name}': ${response.statusText}`,
    );
  }
  return response.json();
}

function findPureWheel(
  info: PyPIInfo,
): { filename: string; url: string } | null {
  // Look for py3-none-any wheel first, then py2.py3-none-any
  for (const url of info.urls) {
    if (
      url.packagetype === "bdist_wheel" &&
      (url.filename.includes("-py3-none-any") ||
        url.filename.includes("-py2.py3-none-any"))
    ) {
      return { filename: url.filename, url: url.url };
    }
  }
  return null;
}

const textDecoder = new TextDecoder();

async function downloadAndExtractWheel(
  url: string,
  signal?: AbortSignal,
): Promise<Array<{ path: string; contents: string }>> {
  const response = await fetch(url, { signal });
  if (!response.ok) {
    throw new Error(`Failed to download wheel: ${response.statusText}`);
  }

  const buffer = await response.arrayBuffer();
  const zipData = new Uint8Array(buffer);
  const unzipped = unzipSync(zipData);

  const files: Array<{ path: string; contents: string }> = [];

  for (const [path, data] of Object.entries(unzipped)) {
    // Only extract Python source files and type stubs
    if (
      path.endsWith(".py") ||
      path.endsWith(".pyi") ||
      path.endsWith("/py.typed")
    ) {
      // Skip .dist-info directory files
      if (path.includes(".dist-info/")) {
        continue;
      }
      // The wheel internal structure maps directly to Python package structure
      // e.g., requests/api.py stays as requests/api.py
      files.push({
        path,
        contents: textDecoder.decode(data),
      });
    }
  }

  return files;
}

function normalizePackageName(name: string): string {
  return name.toLowerCase().replace(/[-_.]+/g, "-");
}

/**
 * Parse mandatory dependencies from requires_dist, excluding extras.
 */
function parseMandatoryDeps(requiresDist: string[] | null): string[] {
  if (requiresDist == null) {
    return [];
  }

  const deps: string[] = [];

  for (const spec of requiresDist) {
    // Skip entries with extra conditions like: 'foo ; extra == "dev"'
    if (/extra\s*==/.test(spec)) {
      continue;
    }
    // Extract the package name (before any version specifier or semicolon)
    const match = spec.match(/^([A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?)/);
    if (match) {
      deps.push(normalizePackageName(match[1]));
    }
  }

  return deps;
}

/**
 * Resolve all transitive dependencies using BFS with depth limit.
 */
async function resolveAllDeps(
  packages: string[],
  signal?: AbortSignal,
): Promise<ResolvedPackage[]> {
  const resolved: ResolvedPackage[] = [];
  const seen = new Set<string>();

  interface QueueEntry {
    name: string;
    version: string | null;
    depth: number;
  }

  const queue: QueueEntry[] = packages.map((spec) => {
    const parsed = parseDependencySpec(spec);
    return {
      name: normalizePackageName(parsed.name),
      version: parsed.version,
      depth: 0,
    };
  });

  while (queue.length > 0) {
    if (signal?.aborted) {
      return resolved;
    }

    const entry = queue.shift()!;

    if (seen.has(entry.name)) {
      continue;
    }

    let info: PyPIInfo;
    try {
      info = await fetchPackageInfo(entry.name, entry.version, signal);
    } catch (error) {
      // If this is a top-level package, propagate the error
      if (entry.depth === 0) {
        throw error;
      }
      // For transitive deps, just skip if not found
      continue;
    }

    const wheel = findPureWheel(info);
    if (wheel == null) {
      if (entry.depth === 0) {
        throw new Error(
          `No pure-Python wheel available for '${entry.name}'. ` +
            `Only pure-Python packages (py3-none-any wheels) are supported.`,
        );
      }
      // Transitive dep without pure wheel — skip
      continue;
    }

    seen.add(entry.name);
    resolved.push({
      name: info.info.name,
      version: info.info.version,
      wheelUrl: wheel.url,
      requiresDist: info.info.requires_dist,
    });

    // Enqueue transitive dependencies (with depth limit)
    if (entry.depth < MAX_DEPENDENCY_DEPTH) {
      const deps = parseMandatoryDeps(info.info.requires_dist);
      for (const dep of deps) {
        if (!seen.has(dep)) {
          queue.push({
            name: dep,
            version: null,
            depth: entry.depth + 1,
          });
        }
      }
    }
  }

  return resolved;
}

export function formatError(error: unknown): string {
  const message = error instanceof Error ? error.message : `${error}`;
  return message.startsWith("Error: ")
    ? message.slice("Error: ".length)
    : message;
}
