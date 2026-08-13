import type { FileId } from "../Playground";
import type { FileHandle } from "ty_wasm";

interface Props {
  currentVendoredFile: FileHandle;
  selectedFile: { id: FileId; name: string };
  onBackToUserFile: () => void;
}

export default function VendoredFileBanner({
  currentVendoredFile,
  selectedFile,
  onBackToUserFile,
}: Props) {
  return (
    <div className="bg-blue-50 dark:bg-blue-900 px-3 py-2 border-b border-blue-200 dark:border-blue-700 text-sm">
      <div className="flex items-center justify-between">
        <div>
          <span className="font-medium text-blue-800 dark:text-blue-200">
            Viewing standard library file:
          </span>{" "}
          <code className="font-mono text-blue-700 dark:text-blue-300">
            {currentVendoredFile.path()}
          </code>
          <span className="text-blue-600 dark:text-blue-400 ml-2 text-xs">
            (read-only)
          </span>
        </div>
        <button
          onClick={onBackToUserFile}
          className="px-3 py-1 text-xs bg-blue-100 dark:bg-blue-800 text-blue-800 dark:text-blue-200 rounded border border-blue-300 dark:border-blue-600 hover:bg-blue-200 dark:hover:bg-blue-700 transition-colors"
        >
          Back to {selectedFile.name}
        </button>
      </div>
    </div>
  );
}
