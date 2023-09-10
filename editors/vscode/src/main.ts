/* --------------------------------------------------------------------------------------------
 * Copyright (c) Microsoft Corporation. All rights reserved.
 * Licensed under the MIT License. See License.txt in the project root for license information.
 * ------------------------------------------------------------------------------------------ */

import { ExtensionContext, window, workspace } from "vscode";

import {
  DocumentSelector,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

export async function activate(_context: ExtensionContext) {
  const outputChannel = window.createOutputChannel("Ruff");
  const traceOutputChannel = window.createOutputChannel("Ruff Trace");

  outputChannel.appendLine("Starting Ruff LSP Client");

  const configuration = workspace.getConfiguration("ruff");

  const binary =
    configuration.get<string>("lspBin") ??
    "/home/micha/astral/ruff/target/debug/ruff";

  const logLevel = configuration.get<string>("trace.server");

  const args = ["lsp"];

  if (logLevel == "verbose") {
    args.push("--verbose");
  }

  // If the extension is launched in debug mode then the debug server options are used
  // Otherwise the run options are used
  const serverOptions: ServerOptions = {
    command: binary,
    args,
  };

  const documentSelector: DocumentSelector = [
    { language: "python", scheme: "file" },
  ];

  // Options to control the language client
  const clientOptions: LanguageClientOptions = {
    documentSelector: documentSelector,
    outputChannel,
    traceOutputChannel,
    stdioEncoding: "utf8",
  };

  // Create the language client and start the client.
  client = new LanguageClient("ruff", "Ruff", serverOptions, clientOptions);

  // Start the client. This will also launch the server
  try {
    await client.start();
  } catch (error) {
    client = null;
    outputChannel.show();
    outputChannel.appendLine(`Failed to start LSP Client: ${error}`);
  }
}

export async function deactivate(): Promise<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
