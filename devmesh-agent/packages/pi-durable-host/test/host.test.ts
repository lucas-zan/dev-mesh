import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { DurableHost } from "../src/host.ts";

const request = (
  method: string,
  args: Record<string, unknown>,
  work: string,
  session: string,
  epoch: string | null = null,
) => ({
  type: "request",
  request_id: randomUUID(),
  method,
  work_id: work,
  session_id: session,
  expected_epoch: epoch,
  args,
});

test("real SDK root persists across reopen; idle steer never starts a new run", async () => {
  const dir = await mkdtemp(join(tmpdir(), "devmesh-host-"));
  const work = randomUUID(),
    session = randomUUID();
  try {
    const host = new DurableHost(() => {});
    const started = await host.handle(
      request(
        "start",
        {
          cwd: dir,
          issue_ref: {},
          initial_prompt: "",
          runtime_config_ref: "default",
        },
        work,
        session,
      ),
    );
    assert.equal(started.ok, true);
    assert.equal(started.result.state, "idle");
    const native = started.result.native_session_id,
      instance = started.result.runtime_instance_id;
    const steer = await host.handle(
      request(
        "steer",
        { command_id: randomUUID(), message: "must not start" },
        work,
        session,
        "0",
      ),
    );
    assert.equal(steer.ok, false);
    assert.equal(steer.error.code, "invalid_state");
    const status = await host.handle(request("status", {}, work, session));
    assert.equal(status.result.state, "idle");
    await host.close();
    const reopened = new DurableHost(() => {});
    const restored = await reopened.handle(
      request(
        "resume",
        { workspace: dir, native_session_id: native, checkpoint_ref: null },
        work,
        session,
      ),
    );
    assert.equal(restored.ok, true);
    assert.equal(restored.result.native_session_id, native);
    assert.notEqual(restored.result.runtime_instance_id, instance);
    await reopened.close();
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("wrong Work and stale epoch cannot access a bound Host", async () => {
  const dir = await mkdtemp(join(tmpdir(), "devmesh-host-"));
  const host = new DurableHost(() => {}),
    work = randomUUID(),
    session = randomUUID();
  try {
    await host.handle(
      request(
        "start",
        {
          cwd: dir,
          issue_ref: {},
          initial_prompt: "",
          runtime_config_ref: "default",
        },
        work,
        session,
      ),
    );
    const foreign = await host.handle(
      request(
        "prompt",
        { command_id: randomUUID(), message: "x" },
        randomUUID(),
        session,
        "0",
      ),
    );
    assert.equal(foreign.ok, false);
    assert.equal(foreign.error.code, "invalid_target");
    const stale = await host.handle(
      request(
        "prompt",
        { command_id: randomUUID(), message: "x" },
        work,
        session,
        "99",
      ),
    );
    assert.equal(stale.ok, false);
    assert.equal(stale.error.code, "stale_turn");
  } finally {
    await host.close();
    await rm(dir, { recursive: true, force: true });
  }
});

test("durable tools forward arguments through the trusted local MCP boundary", async () => {
  const { createDevMeshTools } = await import("../src/devmesh-tools.ts");
  const calls: unknown[] = [];
  const extension = createDevMeshTools(async (name, args) => {
    calls.push({ name, args });
    return { content: [{ type: "text", text: "verified" }] };
  });
  const tool = extension.tools!.find(
    (tool) => tool.name === "work_report_delivery",
  )!;
  const result = await tool.execute(
    { candidate_sha: "a".repeat(40) },
    {} as never,
    {} as never,
  );
  assert.deepEqual(calls, [
    { name: "work_report_delivery", args: { candidate_sha: "a".repeat(40) } },
  ]);
  assert.deepEqual(result, { content: [{ type: "text", text: "verified" }] });
  assert.ok(extension.tools!.some((tool) => tool.name === "issue_create"));
});

test("configured approval deadline equals native sleep expiry", async () => {
  const { approvalWindow } = await import("../src/host.ts");
  const window = approvalWindow(17, 1000);
  assert.equal(window.until, 18000);
  assert.equal(Date.parse(window.deadline), window.until);
  assert.throws(() => approvalWindow(0, 1000));
  assert.throws(() => approvalWindow(Number.NaN, 1000));
});
