// The client of `sg lsp` for VS Code (choice 1376): it starts the language server of `sg` for
// files of SG and shows what it finds. Plain JavaScript, so there is nothing to build.
"use strict";

const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

function activate(context) {
    // `sg.path` names the program; without it, `sg` is looked for on the PATH. A workspace that
    // is not trusted cannot set it (`restrictedConfigurations` in package.json).
    const configured = vscode.workspace.getConfiguration("sg").get("path");
    const command = typeof configured === "string" && configured.trim() !== "" ? configured.trim() : "sg";
    const server = { command, args: ["lsp"] };
    client = new LanguageClient("sg", "SG", { run: server, debug: server }, {
        documentSelector: [{ scheme: "file", language: "sg" }],
    });
    client.start().catch((error) => {
        vscode.window.showErrorMessage(`SG: \`${command} lsp\` did not start (${error.message}). Set "sg.path" to the sg program.`);
    });
    context.subscriptions.push({ dispose: () => client && client.stop() });
}

function deactivate() {
    return client ? client.stop() : undefined;
}

module.exports = { activate, deactivate };
