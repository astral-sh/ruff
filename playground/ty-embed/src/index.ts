import { EmbeddableEditor, EditorOptions } from "./EmbeddableEditor";

export interface TyEditorOptions extends EditorOptions {
  container: HTMLElement | string;
}

/**
 * Initialize a ty editor instance in the specified container.
 *
 * @param options Configuration options for the editor
 * @returns A handle to control the editor instance
 *
 * @example
 * ```javascript
 * import { createTyEditor } from 'ty-embed';
 *
 * const editor = createTyEditor({
 *   container: '#editor',
 *   initialCode: 'print("Hello, ty!")',
 *   theme: 'dark',
 *   height: '500px'
 * });
 * ```
 */
export function createTyEditor(options: TyEditorOptions) {
  const editor = new EmbeddableEditor(options.container, options);

  return {
    /**
     * Unmount and cleanup the editor instance
     */
    dispose() {
      editor.dispose();
    },
  };
}

/**
 * Initialize multiple ty editor instances at once.
 *
 * @param selector CSS selector for containers (e.g., '.ty-editor')
 * @param defaultOptions Default options to apply to all editors
 * @returns Array of editor handles
 *
 * @example
 * ```html
 * <div class="ty-editor" data-code="print('Editor 1')"></div>
 * <div class="ty-editor" data-code="print('Editor 2')"></div>
 *
 * <script>
 *   createTyEditors('.ty-editor', { theme: 'dark' });
 * </script>
 * ```
 */
export function createTyEditors(
  selector: string,
  defaultOptions: Partial<EditorOptions> = {},
) {
  const containers = document.querySelectorAll(selector);
  const editors = [];

  for (const container of Array.from(containers)) {
    const dataCode = container.getAttribute("data-code");
    const dataTheme = container.getAttribute("data-theme");
    const dataHeight = container.getAttribute("data-height");
    const dataFile = container.getAttribute("data-file");

    const options: TyEditorOptions = {
      container: container as HTMLElement,
      ...defaultOptions,
      initialCode: dataCode ?? defaultOptions.initialCode,
      theme:
        (dataTheme as "light" | "dark") ?? defaultOptions.theme ?? "light",
      height: dataHeight ?? defaultOptions.height ?? "400px",
      fileName: dataFile ?? defaultOptions.fileName,
    };

    editors.push(createTyEditor(options));
  }

  return editors;
}

// Re-export types
export type { EditorOptions };
