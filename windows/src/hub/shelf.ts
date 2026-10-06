// The Hub's Shelf: the files dropped on the island (the inbox), with what to do with them —
// open one, hand it to Jinx or to Claude Code, upload it (to your school's OneDrive through
// Jinx's own upload, or to your Google Drive), or take it off. Uploads are direct: no model
// is asked anything. Nothing is read from the files here.

import { Bridge, onEvent } from "../core/bridge";
import { JINX_ID, State } from "../core/state";
import { h, clear } from "../views/dom";
import type { HubContext, HubHost, HubTool } from "./types";

interface Item {
  name: string;
  path: string;
  size: number;
  ageSecs: number;
}

interface Uploaded {
  location: string;
  url: string | null;
}

const size = (b: number) => (b < 1024 ? `${b} B` : b < 1048576 ? `${(b / 1024).toFixed(0)} KB` : `${(b / 1048576).toFixed(1)} MB`);
const age = (s: number) => (s < 90 ? "just now" : s < 5400 ? `${Math.round(s / 60)} min ago` : s < 129600 ? `${Math.round(s / 3600)} h ago` : `${Math.round(s / 86400)} d ago`);
const msg = (e: unknown) => String(e).replace(/^Error:\s*/, "");

function build(ctx: HubContext): HubHost {
  const list = h("div", { class: "sh-list" });
  const count = h("div", { class: "hub-hint", text: "" });
  const note = h("div", { class: "hub-err" });
  const folder = h("button", { class: "hub-btn sm", text: "Open folder" });
  const add = h("input", {
    type: "text", class: "sh-add", placeholder: "Add a file: paste its path and press Enter", spellcheck: "false",
  }) as HTMLInputElement;
  const main = h("div", { class: "sh" }, h("div", { class: "sh-head" }, count, folder), list, h("div", { class: "sh-foot" }, add), note);
  const up = h("div", { class: "sh-up" });
  const el = h("div", { class: "sh-wrap" }, main, up);
  up.style.display = "none";

  let items: Item[] = [];
  let key = "";
  let timer: number | null = null;

  /** Hand a shelf file to the chat: it rides with the first message, to whoever is asked. */
  function toChat(item: Item, who: "jinx" | "claude") {
    State.droppedFile = { name: item.name, path: item.path };
    State.promptContext = { kind: "file", name: item.name, path: item.path };
    State.jinxFilePath = null;
    State.agentFilePath = null;
    if (who === "jinx") {
      ctx.actions.talkToJinx();
    } else {
      State.chatTarget = State.agents.length > 0 ? "agent" : "new";
      if (State.chatTarget === "agent") State.agentId = State.agentId ?? State.agents[0].id;
      ctx.actions.setView("prompt");
    }
  }

  // ── Upload ────────────────────────────────────────────────────────────────

  let loginLines: string[] = [];
  let loginText: HTMLElement | null = null;
  void onEvent<{ kind: string; text: string; ok?: boolean }>("school-login", (e) => {
    if (e.kind === "line") {
      if (e.text.trim()) loginLines = [...loginLines.slice(-5), e.text.trim()];
    } else {
      loginLines = [...loginLines, e.ok ? "✓ Signed in. Try the upload again." : `✗ ${e.text || "The sign-in did not finish"}`];
    }
    if (loginText) loginText.textContent = loginLines.join("\n");
  });

  async function openUpload(item: Item) {
    clear(up);
    loginLines = [];
    const title = h("div", { class: "sh-up-title", text: `Send “${item.name}” to…` });
    const status = h("div", { class: "sh-up-status", text: "" });
    const login = h("div", { class: "sh-login" });
    loginText = login;
    const schoolFolder = h("input", { type: "text", class: "sh-folder", spellcheck: "false", value: State.settings.schoolFolder || "Por clasificar", placeholder: "Materias/BIOLOGIA I/Actividades" }) as HTMLInputElement;
    const driveFolder = h("input", { type: "text", class: "sh-folder", spellcheck: "false", value: State.settings.driveFolder || "Coucou", placeholder: "Coucou" }) as HTMLInputElement;
    const school = h("button", { class: "hub-btn primary", text: "School OneDrive" });
    const drive = h("button", { class: "hub-btn primary", text: "My Google Drive" });
    const cancel = h("button", { class: "hub-btn sm", text: "Cancel" });
    const signIn = h("button", { class: "hub-btn", text: "Sign in again" });
    signIn.style.display = "none";
    let busy = false;
    let connected = false;

    const lock = (on: boolean) => {
      busy = on;
      for (const b of [school, drive, signIn]) (b as HTMLButtonElement).disabled = on;
    };

    async function refreshDrive() {
      const st = await Bridge.driveState().catch(() => null);
      connected = !!st?.connected;
      drive.textContent = !st?.installed ? "My Google Drive (rclone missing)" : connected ? "My Google Drive" : "Connect Google Drive…";
      (drive as HTMLButtonElement).disabled = busy || !st?.installed;
    }
    void refreshDrive();

    async function send(dest: "school" | "drive", folderValue: string) {
      lock(true);
      signIn.style.display = "none";
      status.className = "sh-up-status";
      status.textContent = dest === "school" ? "Sending to the server, then to OneDrive…" : "Uploading to Google Drive…";
      try {
        const r = (await Bridge.shelfUpload(item.path, dest, folderValue)) as Uploaded;
        status.textContent = `✓ ${dest === "school" ? "School OneDrive: " : ""}${r.location}`;
        status.classList.add("ok");
        if (dest === "school") State.settings.schoolFolder = folderValue;
        else State.settings.driveFolder = folderValue;
        void Bridge.saveSettings(State.settings);
        if (r.url) {
          const open = h("button", { class: "hub-btn sm", text: "Open" });
          open.addEventListener("click", () => void Bridge.openUrl(r.url!));
          status.append(" ", open);
        }
      } catch (e) {
        const m = msg(e);
        if (m === "reauth") {
          status.textContent = "The school sign-in has expired (Microsoft asks again every so often). Sign in once and try again.";
          status.classList.add("bad");
          signIn.style.display = "";
        } else {
          status.textContent = m;
          status.classList.add("bad");
        }
      } finally {
        lock(false);
        void refreshDrive();
      }
    }

    school.addEventListener("click", () => void send("school", schoolFolder.value.trim()));
    drive.addEventListener("click", async () => {
      if (busy) return;
      if (connected) return void send("drive", driveFolder.value.trim());
      lock(true);
      status.className = "sh-up-status";
      status.textContent = "A browser tab opens for Google's sign-in (only files made by Coucou are reachable)…";
      try {
        await Bridge.driveConnect();
        status.textContent = "✓ Google Drive connected. Send the file again.";
        status.classList.add("ok");
      } catch (e) {
        status.textContent = msg(e);
        status.classList.add("bad");
      } finally {
        lock(false);
        void refreshDrive();
      }
    });
    signIn.addEventListener("click", async () => {
      loginLines = [];
      login.textContent = "";
      try {
        await Bridge.schoolLoginStart();
        status.textContent = "Open the page below and enter the code; approve the number in your Authenticator.";
      } catch (e) {
        status.textContent = msg(e);
        status.classList.add("bad");
      }
    });
    cancel.addEventListener("click", () => {
      up.style.display = "none";
      main.style.display = "";
      loginText = null;
    });
    for (const f of [schoolFolder, driveFolder]) f.addEventListener("keydown", (e) => e.stopPropagation());

    up.append(
      h("div", { class: "sh-head" }, title, cancel),
      h("div", { class: "sh-dest" }, school, schoolFolder),
      h("div", { class: "sh-dest" }, drive, driveFolder),
      h("div", { class: "sh-dest" }, status, signIn),
      login,
    );
    main.style.display = "none";
    up.style.display = "";
  }

  // ── The list ──────────────────────────────────────────────────────────────

  function render() {
    const jinxOn = State.tasks.some((t) => t.id === JINX_ID);
    const next = JSON.stringify([items.map((i) => [i.path, Math.floor(i.ageSecs / 60)]), jinxOn]);
    if (next === key) return;
    key = next;
    count.textContent = items.length === 0 ? "Nothing on the shelf. Drop a file on the island (+) or add one by path." : `${items.length} file${items.length === 1 ? "" : "s"} · kept for a week`;
    clear(list);
    for (const it of items.slice(0, 20)) {
      const open = h("button", { class: "hub-btn sm", text: "Open" });
      open.addEventListener("click", () => void Bridge.shelfOpen(it.path).catch((e) => (note.textContent = msg(e))));
      const send = h("button", { class: "hub-btn sm", text: "Upload", title: "School OneDrive or Google Drive" });
      send.addEventListener("click", () => void openUpload(it));
      const claude = h("button", { class: "hub-btn sm", text: "Claude" });
      claude.addEventListener("click", () => toChat(it, "claude"));
      const del = h("button", { class: "hub-btn sm", text: "✕", title: "Remove from the shelf" });
      del.addEventListener("click", async () => {
        try {
          await Bridge.shelfRemove(it.path);
        } catch (e) {
          note.textContent = msg(e);
        }
        await refresh();
      });
      const row = h("div", { class: "sh-row" }, h("span", { class: "sh-name", text: it.name, title: it.path }), h("span", { class: "sh-meta", text: `${size(it.size)} · ${age(it.ageSecs)}` }), open, send);
      if (jinxOn) {
        const jinx = h("button", { class: "hub-btn sm", text: "Jinx" });
        jinx.addEventListener("click", () => toChat(it, "jinx"));
        row.append(jinx);
      }
      row.append(claude, del);
      list.append(row);
    }
  }

  async function refresh() {
    try {
      items = (await Bridge.shelfList()) ?? [];
    } catch {
      items = [];
    }
    key = "";
    render();
  }

  folder.addEventListener("click", () => void Bridge.shelfOpen(null));
  add.addEventListener("keydown", async (e) => {
    e.stopPropagation(); // Escape closes the island, not the field
    if (e.key !== "Enter") return;
    const path = add.value.trim().replace(/^'(.*)'$/, "$1").replace(/^file:\/\//, "");
    if (!path) return;
    note.textContent = "";
    try {
      await Bridge.ingestFile(path);
      add.value = "";
    } catch (err) {
      note.textContent = msg(err);
    }
    await refresh();
  });

  return {
    el,
    sync: render,
    focus: () => (up.style.display === "none" ? add.focus() : undefined),
    start() {
      void refresh();
      timer = window.setInterval(() => void refresh(), 4000);
    },
    stop() {
      if (timer != null) window.clearInterval(timer);
      timer = null;
    },
  };
}

export const shelfTool: HubTool = { id: "shelf", label: "Shelf", typing: true, build };
