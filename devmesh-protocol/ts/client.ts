/** REST client shared by Web and future App clients. Admission never implies application. */
export class RelayError extends Error {
  readonly code: string;
  readonly requestId?: string;
  constructor(code: string, message: string, requestId?: string) {
    super(message);
    this.name = "RelayError";
    this.code = code;
    this.requestId = requestId;
  }
}
type Transport = (url: string, options?: RequestInit) => Promise<Response>;
export class RelayClient {
  readonly base: string;
  private readonly transport: Transport;
  private readonly token?: string;
  constructor(
    base = "",
    transport: Transport = (url, options) => globalThis.fetch(url, options),
    token?: string,
  ) {
    this.base = base.replace(/\/$/, "");
    this.transport = transport;
    this.token = token;
  }
  async request<T>(path: string, options: RequestInit = {}): Promise<T> {
    const headers = new Headers(options.headers);
    headers.set("Content-Type", "application/json");
    if (this.token) headers.set("Authorization", `Bearer ${this.token}`);
    const response = await this.transport(`${this.base}/api/v1${path}`, {
      credentials: "same-origin",
      ...options,
      headers,
    });
    const body = await response.json();
    if (!response.ok)
      throw new RelayError(
        body.error?.code ?? "http_error",
        body.error?.message ?? `HTTP ${response.status}`,
        body.request_id,
      );
    return body as T;
  }
  getSession<T>(id: string) {
    return this.request<T>(`/sessions/${encodeURIComponent(id)}`);
  }
  getIssue<T>(id: string) {
    return this.request<T>(`/issues/${encodeURIComponent(id)}`);
  }
  sendCommand<T>(session: string, command: unknown, idempotencyKey: string) {
    return this.request<T>(
      `/sessions/${encodeURIComponent(session)}/commands`,
      {
        method: "POST",
        headers: { "Idempotency-Key": idempotencyKey },
        body: JSON.stringify(command),
      },
    );
  }
  streamUrl(cursor?: string) {
    return `${this.base}/api/v1/events/stream${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ""}`;
  }
}
