// Where the bridge is and the key to it. They are typed once on the phone (the companion page) and kept in the
// Even app's storage, so neither the address nor the key is ever in the build.

export interface Settings { base: string; key: string }
export interface Storage { get(key: string): Promise<string>; set(key: string, value: string): Promise<void> }

const KEY = "jinx-g2-settings";

/** Accepts what a person types: trims, adds https:// when missing, drops trailing slashes. Null if unusable. */
export function normalizeBase(input: string): string | null {
  let v = (input || "").trim();
  if (!v) return null;
  if (!/^https?:\/\//i.test(v)) v = "https://" + v;
  try {
    const u = new URL(v);
    if (!u.hostname) return null;
    return u.origin + u.pathname.replace(/\/+$/, "");
  } catch {
    return null;
  }
}

export async function loadSettings(store: Storage): Promise<Settings | null> {
  try {
    const d = JSON.parse((await store.get(KEY)) || "null");
    const base = normalizeBase(d?.base);
    if (base && typeof d?.key === "string" && d.key.trim()) return { base, key: d.key.trim() };
  } catch { /* sin ajustes todavía */ }
  return null;
}

export async function saveSettings(store: Storage, base: string, key: string): Promise<Settings | null> {
  const b = normalizeBase(base);
  const k = (key || "").trim();
  if (!b || !k) return null;
  await store.set(KEY, JSON.stringify({ base: b, key: k }));
  return { base: b, key: k };
}
