import { connect } from "node:net";
import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { defineExtension, defineTool } from "@earendil-works/pi-durable";
type Args = Record<string, unknown>;
type ToolResult = {
  content: Array<{ type: "text"; text: string }>;
  isError?: boolean;
};
export type LocalCall = (name: string, args: Args) => Promise<ToolResult>;
export const callLocal: LocalCall = async (name, args) => {
  const path = process.env.DEVMESH_SOCKET,
    token = process.env.DEVMESH_WORK_TOKEN;
  if (!path || !token)
    throw new Error("unbound_context: launch this Host through DevMesh");
  return new Promise((resolve, reject) => {
    const socket = connect(path);
    let buffer = "";
    socket.setTimeout(30000, () =>
      socket.destroy(new Error("Local MCP response timed out")),
    );
    socket.once("connect", () =>
      socket.write(
        JSON.stringify({
          token,
          jsonrpc: "2.0",
          id: randomUUID(),
          method: "tools/call",
          params: { name, arguments: args },
        }) + "\n",
      ),
    );
    socket.on("data", (chunk) => {
      buffer += chunk.toString();
      const end = buffer.indexOf("\n");
      if (end < 0) return;
      try {
        const response = JSON.parse(buffer.slice(0, end));
        socket.end();
        if (response.error) reject(new Error(response.error.message));
        else resolve(response.result);
      } catch (error) {
        socket.destroy();
        reject(error);
      }
    });
    socket.once("error", reject);
  });
};
export function createDevMeshTools(call: LocalCall = callLocal) {
  const definitions = JSON.parse(
    readFileSync(
      new URL("../../../resources/tools.json", import.meta.url),
      "utf8",
    ),
  ) as Array<{ name: string; description: string; inputSchema: object }>;
  return defineExtension({
    name: "devmesh",
    tools: definitions.map((tool) =>
      defineTool({
        name: tool.name,
        description: tool.description,
        parameters: tool.inputSchema,
        replay: "unsafe",
        execute: async (args) => call(tool.name, args as Args),
      }),
    ),
  });
}
