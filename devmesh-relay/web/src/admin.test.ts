import test from "node:test";
import assert from "node:assert/strict";
import {
  adminIdentifier,
  adminPayload,
  adminRecordAction,
} from "./admin-model.ts";
test("project creation uses a numeric repository id and stable instance UUID", () => {
  const form = new FormData();
  form.set("name", "team/api");
  form.set("instance_id", "instance");
  form.set("repository_id", "42");
  assert.deepEqual(adminPayload("projects", form), {
    name: "team/api",
    instance_id: "instance",
    repository_id: 42,
  });
});
test("device key creation uses only explicitly selected scopes", () => {
  const form = new FormData();
  form.set("name", "My laptop");
  form.set("user_id", "owner");
  form.append("scopes", "read");
  form.append("scopes", "ingest");
  assert.deepEqual(adminPayload("keys", form), {
    name: "My laptop",
    user_id: "owner",
    scopes: ["read", "ingest"],
  });
});

test("a device shows its full UUID and only offline devices can be removed", () => {
  const id = "51bc3214-46d7-4e81-9254-2e00f57e9cec";
  assert.equal(adminIdentifier("devices", id), id);
  assert.equal(adminIdentifier("users", id), "51bc3214-46d");
  assert.deepEqual(adminRecordAction("devices", { id, online: false }), {
    label: "移除",
    path: `/admin/devices/${id}/registration`,
    disabled: false,
  });
  assert.equal(
    adminRecordAction("devices", { id, online: true })?.disabled,
    true,
  );
  assert.equal(adminRecordAction("keys", { id })?.label, "撤销");
});
