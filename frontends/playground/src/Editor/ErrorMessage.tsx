function truncate(str: string, length: number) {
  if (str.length > length) {
    return str.slice(0, length) + "...";
  } else {
    return str;
  }
}

export function ErrorMessage({ children }: { children: string }) {
  return (
    <div
      className="bg-orange-100 border-l-4 border-orange-500 text-orange-700 p-4"
      role="alert"
    >
      <p className="font-bold">Error</p>
      <p className="block sm:inline">
        {truncate(
          children.startsWith("Error: ")
            ? children.slice("Error: ".length)
            : children,
          120,
        )}
      </p>
    </div>
  );
}
