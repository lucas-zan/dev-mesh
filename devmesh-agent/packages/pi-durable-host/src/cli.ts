import { createInterface } from "node:readline";
import { randomUUID } from "node:crypto";
import { DurableHost } from "./host.ts";
const output = (value: unknown) =>
  process.stdout.write(JSON.stringify(value) + "\n");
const host = new DurableHost(output, {
  approvalTimeoutSeconds: Number(
    process.env.DEVMESH_APPROVAL_TIMEOUT_SECONDS ?? 900,
  ),
});
output({
  type: "hello",
  ipc_version: "1.1",
  runtime: "pi-durable",
  runtime_version: "1.6.0",
  adapter_version: "0.1.0",
  runtime_instance_id: host.instance,
  capabilities: {
    message_events: true,
    tool_events: true,
    steer: true,
    follow_up: false,
    prompt: true,
    interrupt: true,
    local_approval: true,
    local_input: false,
    resume: true,
    child_sessions: false,
  },
});
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const line of lines) {
  try {
    if (Buffer.byteLength(line) > 1048576)
      throw new Error("Frame exceeds 1MiB");
    output(await host.handle(JSON.parse(line)));
  } catch (error) {
    output({
      type: "response",
      request_id: randomUUID(),
      method: "unknown",
      ok: false,
      error: {
        code: "invalid_request",
        message: error instanceof Error ? error.message : "Invalid frame",
        retryable: false,
      },
    });
  }
}
await host.close();
