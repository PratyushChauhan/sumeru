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
