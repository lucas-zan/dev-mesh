import { randomUUID } from "node:crypto";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import { createModels } from "@earendil-works/pi-ai/models";
import { openaiProvider } from "@earendil-works/pi-ai/providers/openai";
import { anthropicProvider } from "@earendil-works/pi-ai/providers/anthropic";
import { googleProvider } from "@earendil-works/pi-ai/providers/google";
import {
  Harness,
  LiveDoc,
  createRegistry,
  defineExtension,
  defineSignal,
  hook,
  ToolTask,
  GenerationTask,
  ConversationIdle,
  ConversationBusy,
} from "@earendil-works/pi-durable";
import type {
  Conversation,
  ConversationView,
  Observation,
  ToolHookCall,
  TaskId,
} from "@earendil-works/pi-durable";
import { createCodingTools } from "@earendil-works/pi-durable/tools";
import { NodeExecutionEnv } from "@earendil-works/pi-durable/env/node";
import { openNodeSqliteStorage } from "@earendil-works/pi-durable/storage/sqlite/node";
import { Journal } from "./journal.ts";
import { createDevMeshTools } from "./devmesh-tools.ts";

type Request = {
  type: string;
  request_id: string;
  method: string;
  work_id: string | null;
  session_id: string | null;
  expected_epoch: string | null;
  args: Record<string, unknown>;
};
type Json = Record<string, unknown>;
type Response =
  | {
      type: "response";
      request_id: string;
      method: string;
      ok: true;
      result: Json;
    }
  | {
      type: "response";
      request_id: string;
      method: string;
      ok: false;
      error: { code: string; message: string; retryable: boolean };
    };
type Binding = {
  work: string;
  session: string;
  epoch: number;
  terminal: boolean;
  busy: boolean;
};
class DomainError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}
const approvalSignal = defineSignal({
  name: "devmesh.approval",
  version: 1,
  schema: {
    type: "object",
    properties: { decision: { enum: ["approve", "deny"] } },
    required: ["decision"],
    additionalProperties: false,
  } as const,
});
const capabilities = {
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
};

export function approvalWindow(seconds: number, now = Date.now()) {
  if (!Number.isSafeInteger(seconds) || seconds <= 0)
    throw new Error("Approval timeout must be a positive integer in seconds");
  const until = now + seconds * 1000;
  return { until, deadline: new Date(until).toISOString() };
}

export class DurableHost {
  readonly instance = randomUUID();
  private emit: (event: Json) => void;
  private harness?: Harness;
  private root?: Conversation;
  private journal?: Journal;
  private binding?: Binding;
  private observation?: Observation<ConversationView>;
  private approvalTimeoutSeconds: number;
  constructor(
    emit: (event: Json) => void,
    options: { approvalTimeoutSeconds?: number } = {},
  ) {
    this.emit = emit;
    this.approvalTimeoutSeconds = options.approvalTimeoutSeconds ?? 900;
    approvalWindow(this.approvalTimeoutSeconds);
  }

  async handle(request: Request): Promise<Response> {
    try {
      const result = await this.dispatch(request);
      return {
        type: "response",
        request_id: request.request_id,
        method: request.method,
        ok: true,
        result,
      };
    } catch (error) {
      const code =
        error instanceof DomainError
          ? error.code
          : error instanceof ConversationIdle ||
              error instanceof ConversationBusy
            ? "invalid_state"
            : "runtime_error";
      return {
        type: "response",
        request_id: request.request_id,
        method: request.method,
        ok: false,
        error: {
          code,
          message: error instanceof Error ? error.message : "Native Host error",
          retryable: false,
        },
      };
    }
  }
  private async dispatch(request: Request): Promise<Json> {
    if (request.method === "probe")
      return { version: "1.6.0", capabilities, health: "ok" };
    if (
      request.method === "start" ||
      request.method === "resume" ||
      request.method === "attach"
    )
      return this.open(request);
    this.checkBinding(request);
    switch (request.method) {
      case "status":
        return this.status();
      case "steer":
      case "prompt":
      case "follow_up":
        return this.submit(request);
      case "interrupt":
      case "stop": {
        await this.root!.abort(BACKGROUND_CONTEXT);
        await this.root!.waitForIdle(BACKGROUND_CONTEXT);
        if (request.method === "stop") {
          this.binding!.terminal = true;
          this.saveBinding();
        }
        return { quiesced: true, outcome: "cancelled" };
      }
      case "resolve_approval":
        return this.resolveApproval(request.args);
      default:
        throw new DomainError("unsupported_capability", request.method);
    }
  }
  private checkBinding(request: Request): void {
    if (!this.binding || !this.root || !this.harness)
      throw new DomainError("invalid_state", "Host has no Work");
    if (
      request.work_id !== this.binding.work ||
      request.session_id !== this.binding.session
    )
      throw new DomainError("invalid_target", "Wrong Work or Session");
    if (this.binding.terminal)
      throw new DomainError(
        "invalid_state",
        "Terminal Work cannot resume or accept control",
      );
    if (
      [
        "steer",
        "prompt",
        "follow_up",
        "interrupt",
        "resolve_approval",
      ].includes(request.method) &&
      request.expected_epoch !== String(this.binding.epoch)
    )
      throw new DomainError("stale_turn", "Run epoch changed");
  }
  private saveBinding(): void {
    this.journal!.set("binding", this.binding);
  }
  private event(type: string, payload: Json): void {
    this.emit({
      type: "event",
      runtime_instance_id: this.instance,
      native_session_id: this.root ? String(this.root.id) : null,
      run_epoch: String(this.binding?.epoch ?? 0),
      event_type: type,
      payload,
    });
  }
  private async open(request: Request): Promise<Json> {
    const cwd = String(request.args.cwd ?? request.args.workspace ?? "");
    if (!request.work_id || !request.session_id || !cwd)
      throw new DomainError(
        "invalid_request",
        "Work, Session and workspace are required",
      );
    if (this.root) {
      this.checkBinding({ ...request, method: "status" });
      return this.status();
    }
    const storage = join(cwd, ".devmesh-native");
    await mkdir(storage, { recursive: true });
    this.journal = new Journal(
      join(storage, `${request.work_id}.journal.sqlite`),
    );
    const stored = this.journal.get<Binding>("binding");
    if (
      stored &&
      (stored.work !== request.work_id ||
        stored.session !== request.session_id ||
        stored.terminal)
    )
      throw new DomainError(
        "invalid_target",
        "Stored Work binding is closed or different",
      );
    this.binding = stored ?? {
      work: request.work_id,
      session: request.session_id,
      epoch: 0,
      terminal: false,
      busy: false,
    };
    this.saveBinding();
    const models = createModels();
    models.setProvider(openaiProvider());
    models.setProvider(anthropicProvider());
    models.setProvider(googleProvider());
    const registry = createRegistry();
    registry.install(createCodingTools());
    registry.install(createDevMeshTools());
    registry.install(
      defineExtension({
        name: "devmesh-observer",
        hooks: [
          hook(GenerationTask, {
            afterResponse: (message) => {
              const text = message.content
                .filter((item) => item.type === "text")
                .map((item) => item.text)
                .join("\n");
              if (text)
                this.event("message.completed", {
                  message_id: randomUUID(),
                  author_kind: "agent",
                  text,
                });
            },
          }),
          hook(ToolTask, {
            beforeTool: async (call, api, context) => {
              this.event("tool.started", {
                tool_call_id: call.id,
                tool: call.name,
                summary: "Tool started",
              });
              if (
                call.name === "read" ||
                !["bash", "write", "edit", "powershell"].includes(call.name)
              )
                return;
              const key = String(api.taskId),
                state = await api.readSignal(approvalSignal, key, context);
              if (state) {
                if ("expired" in state)
                  return { block: "Local approval timed out" };
                return (state.value as { decision: string }).decision ===
                  "approve"
                  ? undefined
                  : { block: "Local user denied execution" };
              }
              this.journal!.set(`approval:${key}`, {
                task: key,
                call: call.id,
                kind: call.name,
              });
              const window = approvalWindow(this.approvalTimeoutSeconds);
              this.event("approval.requested", {
                approval_id: key,
                kind: call.name,
                summary: "Local approval required",
                deadline: window.deadline,
              });
              return {
                sleep: {
                  signals: [approvalSignal.key(key)],
                  until: window.until,
                },
              };
            },
            afterTool: (call, result) => {
              this.event("tool.completed", {
                tool_call_id: call.id,
                tool: call.name,
                summary: "Tool finished",
                outcome: result.isError ? "failed" : "completed",
                duration_ms: 0,
              });
              return undefined;
            },
          }),
        ],
      }),
    );
    this.harness = await Harness.open(
      await openNodeSqliteStorage(join(storage, `${request.work_id}.sqlite`)),
      { models, registry, env: () => new NodeExecutionEnv({ cwd }) },
      BACKGROUND_CONTEXT,
    );
    const model = String(request.args.runtime_config_ref ?? "default");
    const slash = model.indexOf("/");
    this.root = await this.harness.root(
      BACKGROUND_CONTEXT,
      slash > 0
        ? {
            agent: {
              model: {
                provider: model.slice(0, slash),
                modelId: model.slice(slash + 1),
              },
              cwd,
            },
          }
        : undefined,
    );
    if (
      request.method !== "start" &&
      String(request.args.native_session_id) !== String(this.root.id)
    )
      throw new DomainError(
        "invalid_target",
        "Native Conversation ID mismatch",
      );
    this.observation = await this.harness.observeConversation(
      this.root.id,
      {
        advance: (call) => {
          if (call.kind === "value") this.observe(call.value);
        },
        end: () => {},
      },
      BACKGROUND_CONTEXT,
    );
    this.observe(this.observation.value);
    if (
      request.method === "start" &&
      typeof request.args.initial_prompt === "string" &&
      request.args.initial_prompt.length
    ) {
      await this.root.submit(
        {
          type: "input",
          content: request.args.initial_prompt,
          whenBusy: "reject",
        },
        BACKGROUND_CONTEXT,
      );
    }
    if (request.method !== "start") this.harness.resume();
    return this.status();
  }
  private observe(view: ConversationView): void {
    const busy = !!view.docs["pi.live"]?.run;
    if (busy === this.binding!.busy) return;
    this.binding!.busy = busy;
    if (busy) {
      this.binding!.epoch++;
      this.event("turn.started", { run_epoch: String(this.binding!.epoch) });
    } else
      this.event("turn.completed", {
        run_epoch: String(this.binding!.epoch),
        outcome: view.docs["pi.live"]?.runFailure ? "failed" : "completed",
      });
    this.saveBinding();
  }
  private async status(): Promise<Json> {
    const live = await this.harness!.snapshot(
      LiveDoc,
      this.root!.id,
      BACKGROUND_CONTEXT,
    );
    return {
      native_session_id: String(this.root!.id),
      runtime_instance_id: this.instance,
      run_epoch: String(this.binding!.epoch),
      state: this.binding!.terminal ? "ended" : live?.run ? "running" : "idle",
      quiesced: !live?.run,
    };
  }
  private async submit(request: Request): Promise<Json> {
    if (request.method === "follow_up")
      throw new DomainError(
        "unsupported_capability",
        "Follow-up queue is owned by Rust Controller",
      );
    const id = String(request.args.command_id ?? "");
    if (!id) throw new DomainError("invalid_request", "command_id required");
    const key = `command:${id}`,
      fingerprint = JSON.stringify({
        method: request.method,
        args: request.args,
      });
    const old = this.journal!.get<{
      fingerprint: string;
      result?: Json;
      error?: { code: string; message: string };
    }>(key);
    if (old) {
      if (old.fingerprint !== fingerprint)
        throw new DomainError("idempotency_conflict", id);
      if (old.error) throw new DomainError(old.error.code, old.error.message);
      if (old.result) return old.result;
      throw new DomainError(
        "unknown_outcome",
        "Admission outcome unknown; do not replay",
      );
    }
    this.journal!.set(key, { fingerprint });
    try {
      const submission = await this.root!.submit(
        {
          type: "input",
          content: String(request.args.message ?? ""),
          whenBusy: request.method === "steer" ? "steer" : "reject",
          whenIdle: request.method === "steer" ? "reject" : undefined,
        },
        BACKGROUND_CONTEXT,
      );
      const result = {
        disposition: request.method === "steer" ? "queued" : "started",
        native_receipt_ref: String(submission.id),
      };
      this.journal!.set(key, { fingerprint, result });
      return result;
    } catch (error) {
      if (
        error instanceof ConversationIdle ||
        error instanceof ConversationBusy
      )
        this.journal!.set(key, {
          fingerprint,
          error: { code: "invalid_state", message: error.message },
        });
      throw error;
    }
  }
  private async resolveApproval(args: Json): Promise<Json> {
    const id = String(args.approval_id ?? ""),
      pending = this.journal!.get<{ task: TaskId }>(`approval:${id}`);
    if (!pending) throw new DomainError("not_found", "Approval is not active");
    if (args.decision !== "approve" && args.decision !== "deny")
      throw new DomainError("invalid_request", "approve or deny required");
    const result = await this.harness!.signal(
      pending.task,
      approvalSignal,
      id,
      { decision: args.decision },
      BACKGROUND_CONTEXT,
    );
    if (result !== "delivered" && result !== "duplicate")
      throw new DomainError("invalid_state", result);
    this.event("approval.resolved", {
      approval_id: id,
      outcome: args.decision,
    });
    return { accepted: true, native_receipt_ref: id };
  }
  async close(): Promise<void> {
    this.observation?.detach();
    if (this.harness) await this.harness.close(BACKGROUND_CONTEXT);
    this.journal?.close();
  }
}
