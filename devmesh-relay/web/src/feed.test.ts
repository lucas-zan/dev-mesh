import test from "node:test";
import assert from "node:assert/strict";
import { subscribeFeed } from "./feed.ts";

class Source {
  handlers = new Map<string, (event: { lastEventId: string }) => void>();
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  onmessage: ((event: { lastEventId: string }) => void) | null = null;
  closed = false;
  addEventListener(
    type: string,
    handler: (event: { lastEventId: string }) => void,
  ) {
    this.handlers.set(type, handler);
  }
  close() {
    this.closed = true;
  }
  event(cursor: string) {
    this.handlers.get("session.event")!({ lastEventId: cursor });
  }
}
const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
test("snapshot cursor and events arriving during refresh remain continuous", async () => {
  const source = new Source();
  const releases: (() => void)[] = [];
  const feed = subscribeFeed(
    "9007199254740993",
    () => new Promise<void>((resolve) => releases.push(resolve)),
    () => {},
    (url) => {
      assert.equal(url, "/api/v1/events/stream?cursor=9007199254740993");
      return source;
    },
  );
  source.event("9007199254740994");
  source.event("9007199254740995");
  assert.equal(releases.length, 1);
  assert.equal(feed.cursor(), "9007199254740993");
  releases[0]();
  await tick();
  assert.equal(releases.length, 2);
  assert.equal(feed.cursor(), "9007199254740994");
  releases[1]();
  await tick();
  assert.equal(feed.cursor(), "9007199254740995");
  source.event("9007199254740995");
  assert.equal(releases.length, 2);
  feed.close();
  source.event("9007199254740996");
  assert.equal(source.closed, true);
  assert.equal(releases.length, 2);
});
test("failed refresh keeps its cursor and reconnect requests a fresh snapshot", async () => {
  const source = new Source();
  let attempts = 0,
    errors = 0;
  const feed = subscribeFeed(
    "7",
    async () => {
      if (++attempts === 1) throw new Error("unavailable");
    },
    () => errors++,
    () => source,
  );
  source.event("8");
  await tick();
  assert.equal(feed.cursor(), "7");
  assert.equal(errors, 1);
  source.onopen!();
  await tick();
  assert.equal(feed.cursor(), "8");
  assert.equal(attempts, 2);
  feed.close();
});

test("project and catalog notices request a directory update, including on reconnect", async () => {
  const source = new Source();
  const updates: string[][] = [];
  const feed = subscribeFeed(
    "0",
    async (kinds) => {
      updates.push([...kinds]);
    },
    () => {},
    () => source,
  );
  source.handlers.get("project.changed")!({ lastEventId: "1" });
  source.handlers.get("catalog.changed")!({ lastEventId: "2" });
  await tick();
  source.onopen!();
  await tick();
  assert.deepEqual(updates, [
    ["project.changed"],
    ["catalog.changed"],
    ["snapshot"],
  ]);
  feed.close();
});
