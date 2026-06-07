// Tauri IPC client — the desktop replacement for HarnessLab's HTTP REST/SSE
// data layer (`api.ts`). In the desktop app, backend calls go over Tauri
// `invoke(command, args)` and streaming goes over Tauri `listen(event, cb)`
// instead of `fetch`/`EventSource`.
//
// P0 scaffolds this layer with `app_info` + a `parse_agent_config` smoke call,
// proving the frontend ↔ Rust core link. P1+ progressively migrates each
// endpoint in `api.ts` to a matching Tauri command.

import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  name: string;
  version: string;
  engine: string;
}

/** True when running inside the Tauri webview (vs. a plain browser dev server). */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function appInfo(): Promise<AppInfo> {
  return invoke<AppInfo>("app_info");
}

export async function parseAgentConfig(yaml: string): Promise<string> {
  return invoke<string>("parse_agent_config", { yaml });
}
