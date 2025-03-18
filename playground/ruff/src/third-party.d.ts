declare module "lz-string" {
  function decompressFromEncodedURIComponent(
    input: string | null,
  ): string | null;
  function compressToEncodedURIComponent(input: string | null): string;
}
