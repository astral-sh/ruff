import { strToU8, zipSync } from "fflate";

/**
 * Creates a ZIP archive from the given files and triggers a browser download.
 * The filename includes a short content hash for uniqueness.
 */
export async function downloadZip(files: {
  [name: string]: string;
}): Promise<void> {
  const data: { [name: string]: Uint8Array } = {};

  for (const [name, content] of Object.entries(files)) {
    data[name] = strToU8(content);
  }

  const zipped = zipSync(data);

  const hash = await contentHash(JSON.stringify(files));

  const blob = new Blob([zipped.buffer as ArrayBuffer], {
    type: "application/zip",
  });
  const url = URL.createObjectURL(blob);

  const a = document.createElement("a");
  a.href = url;
  a.download = `ty-playground-${hash}.zip`;
  a.click();

  URL.revokeObjectURL(url);
}

async function contentHash(content: string): Promise<string> {
  const encoded = new TextEncoder().encode(content);
  const digest = await crypto.subtle.digest("SHA-256", encoded);
  const bytes = new Uint8Array(digest);
  return Array.from(bytes.slice(0, 4))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}
