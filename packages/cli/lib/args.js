/**
 * Inputs: argv after the `sumeru` binary name (no node/script).
 * Outputs: argv to pass to the native binary (stdio vs desktop).
 */
export function resolveNativeArgs(argv) {
  if (argv.length === 0 || argv[0] === "mcp-stdio") {
    return ["mcp-stdio", ...argv.slice(argv[0] === "mcp-stdio" ? 1 : 0)];
  }
  if (argv[0] === "gui") {
    return argv.slice(1);
  }
  return argv;
}

/**
 * Inputs: argv after `sumeru`, platform (default `process.platform`).
 * Outputs: `["open", args]` to launch the native Sumeru.app for `gui` on macOS (the darwin
 * binary is headless), else null (run the native binary as usual).
 */
export function guiCommand(argv, platform = process.platform) {
  if (platform !== "darwin" || argv[0] !== "gui") return null;
  const rest = argv.slice(1);
  return ["open", ["-b", "com.sumeru.app", ...(rest.length ? ["--args", ...rest] : [])]];
}
