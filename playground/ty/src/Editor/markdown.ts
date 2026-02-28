/**
 * Get the appropriate markdown language tag for a file based on its extension.
 */
function getLanguageTag(filename: string): string {
  const extension = filename.split(".").pop()?.toLowerCase();
  switch (extension) {
    case "py":
      return "python";
    case "pyi":
      return "python";
    case "json":
      return "json";
    case "toml":
      return "toml";
    default:
      return "python"; // Default to python for ty playground
  }
}

/**
 * Generate a markdown link to the playground.
 *
 * @param shareUrl - The shareable playground URL
 * @returns Formatted markdown link string
 */
export function generatePlaygroundMarkdownLink(shareUrl: string): string {
  return `Code sample in [ty playground](${shareUrl})`;
}

/**
 * Generate markdown representation of playground files with a shareable link.
 *
 * @param files - Object mapping filenames to their content
 * @param shareUrl - The shareable playground URL
 * @returns Formatted markdown string
 */
export function generatePlaygroundMarkdown(
  files: { [name: string]: string },
  shareUrl: string,
): string {
  const parts: string[] = [];
  parts.push(generatePlaygroundMarkdownLink(shareUrl));
  parts.push("");

  // Add each file with a heading and code block
  for (const [filename, content] of Object.entries(files)) {
    const language = getLanguageTag(filename);
    parts.push(`### ${filename}`);
    parts.push(`\`\`\`${language}`);
    parts.push(content);
    parts.push("```");
    parts.push("");
  }

  return parts.join("\n").trimEnd();
}
