// The app's side of jinx/g2-bridge. `fetch` is injected so tests (and the simulator) can stand in for the network.

export type FetchLike = (url: string, init?: {
  method?: string; headers?: Record<string, string>; body?: string | Uint8Array; signal?: AbortSignal;
}) => Promise<{ ok: boolean; status: number; json(): Promise<any> }>;

export interface ClientConfig { base: string; key: string }

export interface Reply { id: string; status: "done" | "pending" | "error"; text?: string; error?: string; heard?: string }

export type ErrorKind = "network" | "auth" | "server" | "voice" | "aborted" | "timeout";

export function clientError(kind: ErrorKind, message: string): Error & { kind: ErrorKind } {
  return Object.assign(new Error(message), { kind });
}

/** What to say on the glasses for an error (short: the screen is small). */
export function describe(e: unknown): string {
  const err = e as { kind?: ErrorKind; message?: string };
  switch (err?.kind) {
    case "network": return "Sin conexión con Jinx";
    case "auth": return "Llave incorrecta: revísala en el teléfono";
    case "voice": return err.message || "El servidor no entiende voz todavía";
    case "timeout": return "Jinx tarda mucho; te lo mando a la isla o a Telegram";
    case "aborted": return "";
    default: return err?.message || "Jinx no contestó";
  }
}

const ANSWER_WAIT_S = 25;
const MAX_WAIT_MS = 8 * 60 * 1000;

export function createClient(cfg: ClientConfig, f: FetchLike, now: () => number = Date.now) {
  const base = cfg.base.replace(/\/+$/, "");
  const headers = (extra: Record<string, string> = {}) => ({ Authorization: "Bearer " + cfg.key, ...extra });

  async function call(method: string, path: string, body?: string | Uint8Array, extra: Record<string, string> = {},
                      signal?: AbortSignal) {
    let res;
    try {
      res = await f(base + path, { method, headers: headers(extra), body, signal });
    } catch (e) {
      if (signal?.aborted) throw clientError("aborted", "cancelado");
      throw clientError("network", "sin red");
    }
    if (res.status === 401) throw clientError("auth", "llave");
    let data: any = {};
    try { data = await res.json(); } catch { /* cuerpo vacío o roto */ }
    if (!res.ok) {
      if (path.startsWith("/api/g2/voice") && res.status === 503) throw clientError("voice", data?.error || "sin voz");
      throw clientError("server", data?.error || `error ${res.status}`);
    }
    return data;
  }

  /** Follows a pending question until it is answered, fails, runs out of time or is aborted. */
  async function follow(first: Reply, signal?: AbortSignal): Promise<Reply> {
    const deadline = now() + MAX_WAIT_MS;
    let r = first;
    while (r.status === "pending") {
      if (signal?.aborted) throw clientError("aborted", "cancelado");
      if (now() > deadline) throw clientError("timeout", "tarda");
      const next = await call("GET", `/api/g2/answer?id=${encodeURIComponent(r.id)}&wait=${ANSWER_WAIT_S}`,
        undefined, {}, signal);
      r = { ...next, heard: r.heard };
    }
    if (r.status === "error") throw clientError("server", r.error || "Jinx no contestó");
    return r;
  }

  return {
    async ask(text: string, signal?: AbortSignal): Promise<Reply> {
      const first = await call("POST", "/api/g2/ask", JSON.stringify({ text }), { "Content-Type": "application/json" },
        signal);
      return follow(first, signal);
    },
    /** Raw PCM 16 kHz mono s16le; `onHeard` gets the transcript as soon as the server has it. */
    async voice(pcm: Uint8Array, signal?: AbortSignal): Promise<Reply> {
      const first: Reply = await call("POST", "/api/g2/voice", pcm, { "Content-Type": "application/octet-stream" },
        signal);
      if ((first as any).status === "empty") return first;
      return follow(first, signal);
    },
    async notices(since: number, waitSeconds: number, signal?: AbortSignal): Promise<{ id: number; text: string; source?: string }[]> {
      const d = await call("GET", `/api/g2/notices?since=${since}&wait=${waitSeconds}`, undefined, {}, signal);
      return Array.isArray(d?.notices) ? d.notices : [];
    },
  };
}

export type Client = ReturnType<typeof createClient>;
