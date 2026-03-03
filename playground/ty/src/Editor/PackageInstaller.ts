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
  warnings: string[];
}

export const IDLE_STATUS: InstallationStatus = {
  state: "idle",
  message: "",
  progress: null,
  installedPackages: [],
  writtenFiles: [],
  warnings: [],
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
    warnings: [],
  });

  try {
    // Resolve all transitive dependencies
    const { packages: resolved, warnings } = await resolveAllDeps(
      packages,
      signal,
    );

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
        warnings,
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
      warnings,
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
      warnings: [],
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

interface VersionConstraint {
  op: "==" | "!=" | ">=" | "<=" | ">" | "<" | "~=";
  version: string;
}

interface DependencyWithConstraints {
  name: string;
  constraints: VersionConstraint[];
}

/**
 * Parse version constraint string like ">=2.0,<4.0,!=3.0.1" into individual constraints.
 */
function parseVersionConstraints(specifier: string): VersionConstraint[] {
  if (specifier.length === 0) {
    return [];
  }

  const constraints: VersionConstraint[] = [];
  const parts = specifier.split(",");

  for (const part of parts) {
    const trimmed = part.trim();
    const match = trimmed.match(/^(~=|==|!=|>=|<=|>|<)\s*([A-Za-z0-9.*_-]+)$/);
    if (match) {
      constraints.push({
        op: match[1] as VersionConstraint["op"],
        version: match[2],
      });
    }
  }

  return constraints;
}

/**
 * Compare two PEP 440 version strings numerically.
 * Returns negative if a < b, 0 if equal, positive if a > b.
 */
function compareVersions(a: string, b: string): number {
  const partsA = a.split(".").map((s) => parseInt(s, 10) || 0);
  const partsB = b.split(".").map((s) => parseInt(s, 10) || 0);
  const len = Math.max(partsA.length, partsB.length);

  for (let i = 0; i < len; i++) {
    const numA = partsA[i] ?? 0;
    const numB = partsB[i] ?? 0;
    if (numA !== numB) {
      return numA - numB;
    }
  }
  return 0;
}

/**
 * Check if a resolved version satisfies a single version constraint.
 */
function satisfiesConstraint(
  resolved: string,
  constraint: VersionConstraint,
): boolean {
  // Wildcard matches (e.g. ==3.*) — just check prefix
  if (constraint.version.includes("*")) {
    const prefix = constraint.version.replace(/\.\*$/, "");
    const resolvedPrefix = resolved
      .split(".")
      .slice(0, prefix.split(".").length)
      .join(".");
    const matches = compareVersions(resolvedPrefix, prefix) === 0;
    return constraint.op === "==" ? matches : constraint.op === "!=" ? !matches : true;
  }

  const cmp = compareVersions(resolved, constraint.version);

  switch (constraint.op) {
    case "==":
      return cmp === 0;
    case "!=":
      return cmp !== 0;
    case ">=":
      return cmp >= 0;
    case "<=":
      return cmp <= 0;
    case ">":
      return cmp > 0;
    case "<":
      return cmp < 0;
    case "~=":
      // ~=X.Y is equivalent to >=X.Y,<(X+1).0
      // ~=X.Y.Z is equivalent to >=X.Y.Z,<X.(Y+1).0
      if (cmp < 0) {
        return false;
      }
      {
        const parts = constraint.version.split(".");
        const upperParts = parts.slice(0, -1);
        upperParts[upperParts.length - 1] = String(
          parseInt(upperParts[upperParts.length - 1], 10) + 1,
        );
        return compareVersions(resolved, upperParts.join(".")) < 0;
      }
  }
}

/**
 * Format version constraints as a human-readable string.
 */
function formatConstraints(constraints: VersionConstraint[]): string {
  return constraints.map((c) => `${c.op}${c.version}`).join(",");
}

function normalizePackageName(name: string): string {
  return name.toLowerCase().replace(/[-_.]+/g, "-");
}

/**
 * Parse mandatory dependencies from requires_dist, excluding extras.
 * Returns package names with their version constraints.
 */
function parseMandatoryDeps(
  requiresDist: string[] | null,
): DependencyWithConstraints[] {
  if (requiresDist == null) {
    return [];
  }

  const deps: DependencyWithConstraints[] = [];

  for (const spec of requiresDist) {
    // Skip entries with extra conditions like: 'foo ; extra == "dev"'
    if (/extra\s*==/.test(spec)) {
      continue;
    }
    // Extract the package name (before any version specifier or semicolon)
    const match = spec.match(/^([A-Za-z0-9]([A-Za-z0-9._-]*[A-Za-z0-9])?)/);
    if (match) {
      const name = normalizePackageName(match[1]);
      // Extract version constraints from the remainder (before any semicolon for env markers)
      const afterName = spec.slice(match[0].length).split(";")[0].trim();
      const constraints = parseVersionConstraints(afterName);
      deps.push({ name, constraints });
    }
  }

  return deps;
}

/**
 * Resolve all transitive dependencies using BFS with depth limit.
 */
interface ResolveResult {
  packages: ResolvedPackage[];
  warnings: string[];
}

async function resolveAllDeps(
  packages: string[],
  signal?: AbortSignal,
): Promise<ResolveResult> {
  const resolved: ResolvedPackage[] = [];
  const warnings: string[] = [];
  // Maps normalized name → installed version string
  const seen = new Map<string, string>();

  interface QueueEntry {
    name: string;
    version: string | null;
    constraints: VersionConstraint[];
    depth: number;
    requestedBy: string | null;
  }

  const queue: QueueEntry[] = packages.map((spec) => {
    const parsed = parseDependencySpec(spec);
    return {
      name: normalizePackageName(parsed.name),
      version: parsed.version,
      constraints: parsed.version != null ? [{ op: "==" as const, version: parsed.version }] : [],
      depth: 0,
      requestedBy: null,
    };
  });

  while (queue.length > 0) {
    if (signal?.aborted) {
      return { packages: resolved, warnings };
    }

    const entry = queue.shift()!;
    // entry.name is already normalized (by queue producers)
    const existingVersion = seen.get(entry.name);
    if (existingVersion != null) {
      // Check if the resolved version satisfies the constraints
      for (const constraint of entry.constraints) {
        if (!satisfiesConstraint(existingVersion, constraint)) {
          const source =
            entry.requestedBy != null
              ? ` (required by ${entry.requestedBy})`
              : "";
          warnings.push(
            `${entry.name} ${formatConstraints(entry.constraints)}${source} conflicts with installed ${existingVersion}`,
          );
          break;
        }
      }
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

    seen.set(entry.name, info.info.version);
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
        // Find the exact pin version if there is one (==X.Y.Z)
        const exactPin = dep.constraints.find((c) => c.op === "==" && !c.version.includes("*"));

        if (!seen.has(dep.name)) {
          queue.push({
            name: dep.name,
            version: exactPin?.version ?? null,
            constraints: dep.constraints,
            depth: entry.depth + 1,
            requestedBy: info.info.name,
          });
        } else if (dep.constraints.length > 0) {
          // Already resolved — check constraints against the resolved version
          const resolvedVersion = seen.get(dep.name)!;
          for (const constraint of dep.constraints) {
            if (!satisfiesConstraint(resolvedVersion, constraint)) {
              warnings.push(
                `${dep.name} ${formatConstraints(dep.constraints)} (required by ${info.info.name}) conflicts with installed ${resolvedVersion}`,
              );
              break;
            }
          }
        }
      }
    }
  }

  return { packages: resolved, warnings };
}

export function formatError(error: unknown): string {
  const message = error instanceof Error ? error.message : `${error}`;
  return message.startsWith("Error: ")
    ? message.slice("Error: ".length)
    : message;
}
