import { loadPyodide } from "pyodide";
import { SerializedFiles } from "../Playground";

const SANDBOX_BASE_DIRECTORY = "/playground/";

export async function runPython(workspace: SerializedFiles): Promise<string> {
  let combinedOutput = "";

  const outputHandler = (output: string) => {
    combinedOutput += output + "\n";
  };

  try {
    const pyodide = await loadPyodide({
      env: {
        HOME: SANDBOX_BASE_DIRECTORY,
      },
    });

    pyodide.setStdout({ batched: outputHandler });
    pyodide.setStderr({ batched: outputHandler });

    for (const [fileName, content] of Object.entries(workspace.files)) {
      const lastSeparator = fileName.lastIndexOf("/");

      if (lastSeparator !== -1) {
        const directory =
          SANDBOX_BASE_DIRECTORY + fileName.slice(0, lastSeparator);
        pyodide.FS.mkdirTree(directory);
      }

      pyodide.FS.writeFile(SANDBOX_BASE_DIRECTORY + fileName, content);
    }

    const dict = pyodide.globals.get("dict");
    const globals = dict();

    // Patch `reveal_type` to print runtime values
    try {
      pyodide.runPython(`
        import builtins

        def reveal_type(obj):
          import typing
          print(f"Runtime value is '{obj}'")
          return typing.reveal_type(obj)

        builtins.reveal_type = reveal_type`);

      pyodide.runPython(workspace.files[workspace.current] ?? "", {
        globals,
        locals: globals,
        filename: workspace.current,
      });
    } finally {
      globals.destroy();
      dict.destroy();
    }

    return combinedOutput;
  } catch (error) {
    return `Failed to run Python script: ${error}`;
  }
}
