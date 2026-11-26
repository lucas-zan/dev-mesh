type Notice = { lastEventId: string };
interface FeedSource {
  addEventListener(type: string, listener: (event: Notice) => void): void;
  onopen: ((event: Event) => void) | null;
  onerror: ((event: Event) => void) | null;
  onmessage: ((event: MessageEvent) => void) | null;
  close(): void;
}

/** Snapshot refreshes serialize; notifications received during a refresh stay pending. */
export function subscribeFeed(
  initialCursor: string,
  refresh: (kinds: ReadonlySet<string>) => Promise<void>,
  onError: (error: unknown) => void,
  connect: (url: string) => FeedSource = (url) => new EventSource(url),
) {
  let applied = BigInt(initialCursor),
    requested = applied;
  let running = false,
    closed = false;
  let pending = new Set<string>();
  const source = connect(`/api/v1/events/stream?cursor=${initialCursor}`);
  async function drain() {
    if (running || closed) return;
    running = true;
    try {
      while (pending.size && !closed) {
        const kinds = pending;
        pending = new Set();
        const through = requested;
        try {
          await refresh(kinds);
        } catch (error) {
          for (const kind of kinds) pending.add(kind);
          throw error;
        }
        if (!closed) applied = through;
      }
    } catch (error) {
      if (!closed) onError(error);
    } finally {
      running = false;
    }
  }
  function changed(event: Notice, kind: string) {
    if (closed) return;
    const cursor = BigInt(event.lastEventId);
    if (cursor <= requested) return;
    requested = cursor;
    pending.add(kind);
    void drain();
  }
  for (const kind of [
    "device.changed",
    "session.changed",
    "work.changed",
    "session.event",
    "command.changed",
    "comment.created",
    "issue.changed",
    "catalog.changed",
    "project.changed",
  ])
    source.addEventListener(kind, (event) => changed(event, kind));
  source.onmessage = (event) => changed(event, "snapshot");
  source.onopen = () => {
    if (!closed) {
      pending.add("snapshot");
      void drain();
    }
  };
  source.onerror = () => onError(new Error("实时连接中断，浏览器正在重连。"));
  return {
    cursor: () => applied.toString(),
    close: () => {
      closed = true;
      source.close();
    },
  };
}
