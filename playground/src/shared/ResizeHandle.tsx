import { PanelResizeHandle } from "react-resizable-panels";

export function HorizontalResizeHandle() {
  return (
    <PanelResizeHandle className="cursor-ew-resize w-0.5 bg-gray-200 hover:bg-gray-300"></PanelResizeHandle>
  );
}

export function VerticalResizeHandle() {
  return (
    <PanelResizeHandle className="cursor-eh-resize h-0.5 bg-gray-200 hover:bg-gray-300"></PanelResizeHandle>
  );
}
