import { Separator } from "react-resizable-panels";

export function HorizontalResizeHandle() {
  return (
    <Separator className="cursor-ew-resize w-0.5 bg-gray-200 hover:bg-gray-300"></Separator>
  );
}

export function VerticalResizeHandle() {
  return (
    <Separator className="cursor-eh-resize h-0.5 bg-gray-200 hover:bg-gray-300"></Separator>
  );
}
