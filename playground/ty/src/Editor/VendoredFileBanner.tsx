import type { RefObject } from "react";
import type { editor } from "monaco-editor";
import type { Monaco } from "@monaco-editor/react";
import { FileId, ReadonlyFiles } from "../Playground";

interface Props {
  currentVendoredFile: { path: string; previousFileId: FileId };
  files: ReadonlyFiles;
  editorRef: RefObject<{
    editor: editor.IStandaloneCodeEditor;
    monaco: Monaco;
  } | null>;
  onClearVendoredFile: () => void;
}

export default function VendoredFileBanner({
  currentVendoredFile,
  files,
  editorRef,
  onClearVendoredFile,
}: Props) {
  const handleBackClick = () => {
    // Find the previous file
    const previousFile = files.index.find(
      (f) => f.id === currentVendoredFile.previousFileId,
    );

    // Force the editor to switch back to the user file immediately
    if (previousFile != null && editorRef.current != null) {
      const monaco = editorRef.current.monaco;
      const fileUri = monaco.Uri.file(previousFile.name);

      // Get or create the model for the user file
      let userModel = monaco.editor.getModel(fileUri);
      if (userModel == null) {
        userModel = monaco.editor.createModel(
          files.contents[currentVendoredFile.previousFileId] || "",
          "python",
          fileUri,
        );
      } else {
        // Update the model content
        userModel.setValue(
          files.contents[currentVendoredFile.previousFileId] || "",
        );
      }

      // Force the editor to use this model
      editorRef.current.editor.setModel(userModel);
    }

    // Always clear vendored file state - the selected file remains unchanged
    onClearVendoredFile();
  };

  const backButtonText =
    files.index.find((f) => f.id === currentVendoredFile.previousFileId)
      ?.name || "file";

  return (
    <div className="bg-blue-50 dark:bg-blue-900 px-3 py-2 border-b border-blue-200 dark:border-blue-700 text-sm">
      <div className="flex items-center justify-between">
        <div>
          <span className="font-medium text-blue-800 dark:text-blue-200">
            Viewing standard library file:
          </span>{" "}
          <code className="font-mono text-blue-700 dark:text-blue-300">
            {currentVendoredFile.path}
          </code>
          <span className="text-blue-600 dark:text-blue-400 ml-2 text-xs">
            (read-only)
          </span>
        </div>
        <button
          onClick={handleBackClick}
          className="px-3 py-1 text-xs bg-blue-100 dark:bg-blue-800 text-blue-800 dark:text-blue-200 rounded border border-blue-300 dark:border-blue-600 hover:bg-blue-200 dark:hover:bg-blue-700 transition-colors"
        >
          Back to {backButtonText}
        </button>
      </div>
    </div>
  );
}
