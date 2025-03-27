export function ErrorMessage({ children }: { children: string }) {
  return (
    <div
      className="bg-orange-100 border-l-4 border-orange-500 text-orange-700 p-4"
      role="alert"
    >
      <p className="font-bold">Error</p>
      <p className="block sm:inline">
        {children.startsWith("Error: ")
          ? children.slice("Error: ".length)
          : children}
      </p>
    </div>
  );
}
