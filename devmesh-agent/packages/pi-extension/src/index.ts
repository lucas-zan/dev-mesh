import { connect } from "node:net";
import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { checkSteer } from "./control.ts";

type ToolResult = {
  content: Array<{ type: "text"; text: string }>;
  isError?: boolean;
};
type PiContext = {
  isIdle: () => boolean;
  ui: { notify: (text: string, level?: string) => void };
};
type PiAPI = {
  on: (
    event: string,
    handler: (event: unknown, context: PiContext) => unknown,
  ) => void;
  registerCommand: (
    name: string,
    options: {
      description: string;
      handler: (args: string, context: PiContext) => Promise<void>;
    },
  ) => void;
  registerTool: (tool: {
    name: string;
    label: string;
    description: string;
    parameters: object;
    execute: (id: string, args: Record<string, unknown>) => Promise<ToolResult>;
  }) => void;
  sendUserMessage: (message: string, options: { deliverAs: "steer" }) => void;
  sendMessage: (message: {
    customType: string;
    content: string;
    display: boolean;
    details?: object;
  }) => void;
};

async function callLocal(
  name: string,
  args: Record<string, unknown>,
): Promise<ToolResult> {
  const socketPath = process.env.DEVMESH_SOCKET,
    token = process.env.DEVMESH_WORK_TOKEN;
  if (!socketPath || !token)
    throw new Error("unbound_context: start this Session with DevMesh");
  return new Promise((resolve, reject) => {
    const socket = connect(socketPath);
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
}
export default function devmesh(pi: PiAPI): void {
  let epoch = Number(process.env.DEVMESH_RUN_EPOCH_BASE ?? 0);
  pi.on("agent_start", () => {
    epoch++;
  });
  pi.registerCommand("devmesh-steer", {
    description:
      "DevMesh managed steer; validates idle and epoch in the native host",
    handler: async (args, ctx) => {
      const request = JSON.parse(args) as {
        command_id: string;
        expected_epoch: number;
        message: string;
      };
      try {
        checkSteer(ctx.isIdle(), epoch, request.expected_epoch);
        pi.sendUserMessage(request.message, { deliverAs: "steer" });
        pi.sendMessage({
          customType: "devmesh.control.receipt",
          content: JSON.stringify({
            command_id: request.command_id,
            state: "accepted",
            disposition: "queued",
          }),
          display: false,
        });
      } catch (error) {
        pi.sendMessage({
          customType: "devmesh.control.receipt",
          content: JSON.stringify({
            command_id: request.command_id,
            state: "rejected",
            reason: error instanceof Error ? error.message : "Native rejection",
          }),
          display: false,
        });
      }
    },
  });
  const tools = JSON.parse(
    readFileSync(
      new URL("../../../resources/tools.json", import.meta.url),
      "utf8",
    ),
  ) as Array<{ name: string; description: string; inputSchema: object }>;
  for (const tool of tools) {
    pi.registerTool({
      name: tool.name,
      label: `DevMesh ${tool.name}`,
      description: tool.description,
      parameters: tool.inputSchema,
      execute: async (_id, args) => callLocal(tool.name, args),
    });
  }
}
