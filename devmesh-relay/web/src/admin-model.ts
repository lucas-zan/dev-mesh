import type { RecordData } from "./workspace.ts";

export function adminIdentifier(collection: string, id: string): string {
  return collection === "devices" ? id : id.slice(0, 12);
}

export function adminRecordAction(collection: string, item: RecordData) {
  const id = encodeURIComponent(String(item.id));
  if (collection === "devices")
    return {
      label: "移除",
      path: `/admin/devices/${id}/registration`,
      disabled: item.online === true,
    };
  if (collection === "keys")
    return {
      label: item.revoked === true ? "已撤销" : "撤销",
      path: `/admin/keys/${id}`,
      disabled: item.revoked === true,
    };
}

export function adminPayload(
  collection: string,
  form: FormData,
): Record<string, unknown> {
  const text = (name: string) => String(form.get(name) ?? "");
  if (collection === "projects")
    return {
      name: text("name"),
      instance_id: text("instance_id"),
      repository_id: Number(text("repository_id")),
    };
  if (collection === "users")
    return { username: text("username"), password: text("password") };
  if (collection === "keys")
    return {
      name: text("name"),
      user_id: text("user_id"),
      scopes: form.getAll("scopes").map(String),
    };
  throw new Error("此类型不支持创建");
}
