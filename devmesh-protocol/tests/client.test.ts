import test from "node:test";
import assert from "node:assert/strict";
import { RelayClient, RelayError } from "../ts/client.ts";
test("SDK sends command once with explicit target and idempotency header", async () => {
  let called = 0;
  const client = new RelayClient(
    "https://relay.example.invalid",
    async (input, init) => {
      called++;
      assert.equal(
        input,
        "https://relay.example.invalid/api/v1/sessions/session/commands",
      );
      assert.equal(
        new Headers(init?.headers).get("Idempotency-Key"),
        "stable-id",
      );
      assert.deepEqual(JSON.parse(String(init?.body)), {
        type: "steer",
        target: { session_id: "session" },
      });
      return Response.json(
        { command_id: "command", state: "pending" },
        { status: 202 },
      );
    },
  );
  const command = await client.sendCommand<{ state: string }>(
    "session",
    { type: "steer", target: { session_id: "session" } },
    "stable-id",
  );
  assert.equal(command.state, "pending");
  assert.equal(called, 1);
});
test("SDK exposes Relay error code without replaying a failed request", async () => {
  const client = new RelayClient("", async () =>
    Response.json(
      {
        request_id: "request",
        error: { code: "stale_turn", message: "Run changed", retryable: false },
      },
      { status: 409 },
    ),
  );
  await assert.rejects(
    () => client.request("/sessions/s"),
    (error: unknown) =>
      error instanceof RelayError &&
      error.code === "stale_turn" &&
      error.requestId === "request",
  );
});

test("default browser transport retains the global fetch receiver for login and me", async () => {
  const original = globalThis.fetch;
  const requests: string[] = [];
  globalThis.fetch = async function (this: unknown, input, init) {
    if (this !== globalThis) throw new TypeError("Illegal invocation");
    requests.push(String(input));
    assert.equal(init?.credentials, "same-origin");
    if (String(input).endsWith("/auth/login")) {
      assert.equal(init?.method, "POST");
      assert.deepEqual(JSON.parse(String(init?.body)), {
        username: "admin",
        password: "synthetic",
      });
      return Response.json({ ok: true });
    }
    return Response.json({ username: "admin" });
  };
  try {
    const client = new RelayClient();
    await client.request("/auth/login", {
      method: "POST",
      body: JSON.stringify({ username: "admin", password: "synthetic" }),
    });
    const user = await client.request<{ username: string }>("/me");
    assert.equal(user.username, "admin");
    assert.deepEqual(requests, ["/api/v1/auth/login", "/api/v1/me"]);
  } finally {
    globalThis.fetch = original;
  }
});
