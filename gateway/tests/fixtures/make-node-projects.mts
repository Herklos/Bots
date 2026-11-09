// Writes a projects.json with hbots own Node code, for gateway/src/projects.rs to load.
// Run: node make-node-projects.mts <path-to-hbots>/server <out.json>
import path from "node:path";
const [serverDir, out] = process.argv.slice(2);
const { createProjects } = await import(path.resolve(serverDir, "projects.mts"));
let tick = 0;
const store = createProjects({
  file: path.resolve(out),
  now: () => new Date(Date.UTC(2026, 10, 3, 9, 0, tick++)).toISOString(),
});
const bot = { type: "bot", name: "Ops", slug: "ops" };
const saved = await store.save([
  { ref: "p", kind: "project", title: "hbots", area: "Personal", body: "Parity work.\nSecond line.", fields: { url: "https://example.com/hbots", tags: ["rust", "parity"] }, pinned: true, order: 2 },
  { ref: "m", kind: "machine", title: "NAS", status: "online", fields: { host: "nas.local", owner: "ops" } },
  { kind: "task", title: "Ship the projects page", parentId: "ref:p", fields: { priority: "high", due: "2026-11-05", branch: "main" }, addLinks: [{ type: "thread", threadId: "t-1", label: "chat" }, { type: "bot", slug: "ops" }] },
  { kind: "task", title: "Finished thing", parentId: "ref:p", status: "done" },
  { kind: "service", title: "bot-gateway", machineId: "ref:m", fields: { port: "3780", path: "/srv/bots" }, addLinks: [{ type: "url", url: "https://example.com/status" }] },
  { kind: "repo", title: "bots", machineId: "ref:m", status: "archived", fields: { path: "/home/ops/Bots" }, archived: true },
  { kind: "note", title: "Wifi is on the fridge", body: "Ask before sharing it." },
  { kind: "link", title: "Docs", addLinks: [{ type: "t3project", projectId: "proj-1", environmentId: "env-1" }] },
], bot);
console.log(JSON.stringify({ version: saved.version, items: saved.items.length }));
