// Settings window — the place where anything that writes to disk is confirmed.
// Stage 2 covers the Claude Code hooks and the general preferences; API keys and
// integrations land here too in a later stage.

import "./settings.css";
import { Bridge, onEvent, type HookStatus } from "../core/bridge";
import { DEFAULT_SETTINGS, type Settings } from "../core/state";
import { h, clear } from "../views/dom";

let settings: Settings = { ...DEFAULT_SETTINGS };
let version = "";

const root = document.getElementById("settings-root")!;

async function save() {
  await Bridge.saveSettings(settings);
}

// ── Reusable bits ─────────────────────────────────────────────────────────────

function toggle(on: boolean, onChange: (v: boolean) => void): HTMLElement {
  const el = h("button", { class: on ? "switch on" : "switch", "aria-pressed": on });
  el.addEventListener("click", () => {
    const next = !el.classList.contains("on");
    el.classList.toggle("on", next);
    onChange(next);
  });
  return el;
}

function statusDot(ok: boolean): HTMLElement {
  return h("i", { class: "dot", style: `background:${ok ? "#22c55e" : "#f4505e"}` });
}

function renderDiff(text: string): HTMLElement {
  const box = h("div", { class: "diff" });
  for (const line of text.split("\n")) {
    const cls = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "ctx";
    box.append(h("div", { class: cls, text: line }));
  }
  return box;
}

// ── Claude Code section ───────────────────────────────────────────────────────

function claudeSection(status: HookStatus): HTMLElement {
  const body = h("div", { style: "display:flex;flex-direction:column;gap:12px" });
  const section = h(
    "section",
    {},
    h("h2", {}, statusDot(status.installed), h("span", { text: "Claude Code" })),
    body,
  );

  const rebuild = async () => {
    const fresh = await Bridge.hooksStatus();
    if (fresh) Object.assign(status, fresh);
    clear(body);
    draw();
    const head = section.querySelector("h2")!;
    clear(head);
    head.append(statusDot(status.installed), h("span", { text: "Claude Code" }));
  };

  function draw() {
    body.append(
      h("div", {
        class: "hint",
        text: status.installed
          ? "Coucou is hooked into your Claude Code sessions. Tool calls, questions and permission requests show up in the island, and you can answer them there."
          : "Install the hooks to see your Claude Code sessions in the island and approve permissions without leaving what you are doing.",
      }),
      h("div", { class: "row" },
        h("label", { text: "settings.json" }),
        h("span", { class: "path", text: status.settingsPath }),
      ),
      h("div", { class: "row" },
        h("label", { text: "Relay" }),
        h("span", { class: "path", text: status.hookPath }),
        statusDot(status.hookReady),
      ),
    );

    // Hooks written by an earlier Coucou have no AskUserQuestion entry: the
    // island can show Claude's questions but cannot answer them until they are
    // installed again.
    if (status.installed && !status.askInstalled) {
      body.append(h("div", {
        class: "notice warn",
        text: "Update the hooks to answer Claude's multiple-choice questions from the island: they were installed before that existed.",
      }));
    }

    if (!status.hookReady) {
      body.append(h("div", {
        class: "notice warn",
        text: "coucou-hook.exe is not in place yet. Restart Coucou; if it still fails, build it with `cargo build -p coucou-hook`.",
      }));
    }

    const actions = h("div", { class: "row" });
    const install = h("button", {
      class: "primary",
      text: !status.installed ? "Install hooks…" : status.askInstalled ? "Reinstall hooks…" : "Update hooks…",
      onclick: () => showPreview(true),
    });
    // Writing hook commands that point at a relay which isn't there would give
    // every Claude Code session a broken hook and nothing to show for it.
    if (!status.hookReady) {
      install.disabled = true;
      install.title = "The relay isn't installed yet.";
    }
    actions.append(install);
    if (status.installed) {
      actions.append(h("button", {
        class: "danger",
        text: "Uninstall hooks…",
        onclick: () => showPreview(false),
      }));
    }
    body.append(actions);
  }

  async function showPreview(install: boolean) {
    let preview;
    try {
      preview = await Bridge.hooksPreview(install);
    } catch (err) {
      // An unreadable or invalid settings.json stops here rather than being
      // treated as empty and written over.
      clear(body);
      body.append(
        h("div", { class: "notice err", text: String(err).replace(/^Error:\s*/, "") }),
        h("div", { class: "row" }, h("button", {
          text: "Back",
          onclick: () => { clear(body); draw(); },
        })),
      );
      return;
    }
    if (!preview) return;
    clear(body);
    body.append(
      h("div", {
        class: "hint",
        text: install
          ? "This is exactly what will change in your settings.json. Your own hooks are left untouched."
          : "This removes Coucou's entries only. Your own hooks are left untouched.",
      }),
      renderDiff(preview.diff),
      h("div", { class: "row" },
        h("span", { class: "path", text: `Backup → ${preview.backup}` }),
      ),
    );
    const confirm = h("button", {
      class: install ? "primary" : "danger",
      text: install ? "Back up and write" : "Back up and remove",
    });
    confirm.addEventListener("click", async () => {
      confirm.disabled = true;
      try {
        const backup = await Bridge.hooksApply(install, preview.fingerprint);
        clear(body);
        body.append(h("div", {
          class: "notice ok",
          text: `Done. Previous settings saved as ${backup}. Open a new Claude Code session to pick the hooks up.`,
        }));
        window.setTimeout(() => void rebuild(), 2600);
      } catch (err) {
        confirm.disabled = false;
        body.append(h("div", { class: "notice err", text: `Could not write: ${String(err)}` }));
      }
    });
    body.append(h("div", { class: "row" }, confirm, h("button", {
      text: "Cancel",
      onclick: () => { clear(body); draw(); },
    })));
  }

  draw();
  return section;
}

// ── Claude API section ────────────────────────────────────────────────────────

const MODELS: [string, string][] = [
  ["claude-opus-5", "Claude Opus 5"],
  ["claude-sonnet-5", "Claude Sonnet 5"],
  ["claude-haiku-4-5", "Claude Haiku 4.5"],
];

function apiSection(hasKey: boolean): HTMLElement {
  const dot = statusDot(hasKey);
  const state = h("span", { class: "hint", text: hasKey ? "Key saved in the Windows Credential Manager." : "No key yet — the chat needs one." });

  const field = h("input", {
    type: "password",
    placeholder: hasKey ? "••••••••••••  (stored)" : "sk-ant-...",
    style: "flex:1 1 auto;min-width:0",
    autocomplete: "off",
    spellcheck: "false",
  }) as HTMLInputElement;

  const saveBtn = h("button", { class: "primary", text: "Save key" });
  const clearBtn = h("button", { class: "danger", text: "Remove" });
  const feedback = h("div", {});

  async function refresh() {
    const present = (await Bridge.secretPresent("anthropic-api-key")) ?? false;
    dot.style.background = present ? "#22c55e" : "#f4505e";
    state.textContent = present
      ? "Key saved in the Windows Credential Manager."
      : "No key yet — the chat needs one.";
    field.placeholder = present ? "••••••••••••  (stored)" : "sk-ant-...";
    clearBtn.style.display = present ? "" : "none";
  }

  saveBtn.addEventListener("click", async () => {
    const value = field.value.trim();
    if (!value) return;
    clear(feedback);
    try {
      await Bridge.secretSet("anthropic-api-key", value);
      field.value = "";
      feedback.append(h("div", { class: "notice ok", text: "Saved. It never touches disk." }));
      await refresh();
    } catch (err) {
      feedback.append(h("div", { class: "notice err", text: `Could not save: ${String(err)}` }));
    }
  });

  clearBtn.addEventListener("click", async () => {
    clear(feedback);
    try {
      await Bridge.secretClear("anthropic-api-key");
      feedback.append(h("div", { class: "notice ok", text: "Key removed." }));
      await refresh();
    } catch (err) {
      feedback.append(h("div", { class: "notice err", text: `Could not remove: ${String(err)}` }));
    }
  });

  const model = h("select", {}) as HTMLSelectElement;
  for (const [id, label] of MODELS) model.append(h("option", { value: id, text: label }));
  if (!MODELS.some(([id]) => id === settings.model)) {
    model.append(h("option", { value: settings.model, text: settings.model }));
  }
  model.value = settings.model;
  model.addEventListener("change", () => {
    settings.model = model.value;
    void save();
  });

  clearBtn.style.display = hasKey ? "" : "none";

  return h(
    "section",
    {},
    h("h2", {}, dot, h("span", { text: "Claude" })),
    state,
    h("div", { class: "row" }, h("label", { text: "API key" }), field, saveBtn, clearBtn),
    h("div", { class: "row" }, h("label", { text: "Model" }), model),
    feedback,
  );
}

// ── Integrations section ──────────────────────────────────────────────────────

interface IntegrationDef {
  id: string;
  name: string;
  color: string;
  /** Credential Manager keys, in the order they are shown. */
  fields: { key: string; label: string; placeholder: string; secret: boolean }[];
  /** Shown instead of key fields for a pill that needs none. */
  note?: string;
}

const INTEGRATIONS: IntegrationDef[] = [
  { id: "integration_stripe", name: "Stripe", color: "#0570DE",
    fields: [{ key: "stripe-api-key", label: "Secret key", placeholder: "sk_live_…", secret: true }] },
  { id: "integration_github", name: "GitHub", color: "#F4505E",
    fields: [{ key: "github-token", label: "Token", placeholder: "ghp_…", secret: true }] },
  { id: "integration_vercel", name: "Vercel", color: "#7C5CFF",
    fields: [{ key: "vercel-token", label: "Token", placeholder: "…", secret: true }] },
  { id: "integration_n8n", name: "n8n", color: "#F29B38",
    fields: [
      { key: "n8n-url", label: "Instance URL", placeholder: "https://n8n.example.com", secret: false },
      { key: "n8n-api-key", label: "API key", placeholder: "…", secret: true },
    ] },
  { id: "integration_resend", name: "Resend", color: "#22C55E",
    fields: [{ key: "resend-api-key", label: "API key", placeholder: "re_…", secret: true }] },
  { id: "integration_notion", name: "Notion", color: "#8C8C8C",
    fields: [{ key: "notion-api-key", label: "Integration token", placeholder: "ntn_…", secret: true }] },
  { id: "integration_calcom", name: "Cal.com", color: "#C9956A",
    fields: [{ key: "calcom-api-key", label: "API key", placeholder: "cal_…", secret: true }] },
  { id: "integration_jinx", name: "Jinx", color: "#39FF14",
    fields: [
      { key: "jinx-url", label: "Hermes address", placeholder: "http://100.x.y.z:8642", secret: false },
      { key: "jinx-api-key", label: "API key", placeholder: "the gateway's API_SERVER_KEY", secret: true },
    ],
    note: "Talk to Jinx from the island: her replies and her permission requests show up here. " +
      "Both values are stored in the Secret Service, never on disk." },
  { id: "integration_music", name: "Music", color: "#FA2D48", fields: [],
    note: "What is playing — Spotify, a browser tab, VLC… No key needed. Options are under Music below." },
];

const MAX_ACTIVE = 4;

function integrationsSection(present: Record<string, boolean>): HTMLElement {
  const note = h("div", { class: "hint" });
  const list = h("div", { style: "display:flex;flex-direction:column;gap:14px" });

  function updateNote() {
    const used = settings.activeIntegrations.length;
    note.textContent = `Pick up to ${MAX_ACTIVE} pills to show next to Mochi — ${used}/${MAX_ACTIVE} in use. Keys are stored in the Windows Credential Manager, never on disk.`;
  }

  for (const def of INTEGRATIONS) {
    const active = settings.activeIntegrations.includes(def.id);
    const sw = h("button", { class: active ? "switch on" : "switch" });
    sw.addEventListener("click", () => {
      const on = settings.activeIntegrations.includes(def.id);
      if (on) {
        settings.activeIntegrations = settings.activeIntegrations.filter((x) => x !== def.id);
      } else {
        if (settings.activeIntegrations.length >= MAX_ACTIVE) return;
        settings.activeIntegrations = [...settings.activeIntegrations, def.id];
      }
      sw.classList.toggle("on", !on);
      updateNote();
      void save();
    });

    const rows = h("div", { style: "display:flex;flex-direction:column;gap:6px;flex:1 1 auto;min-width:0" });
    if (def.note) rows.append(h("div", { class: "hint", style: "padding-top:4px", text: def.note }));
    for (const field of def.fields) {
      const input = h("input", {
        type: field.secret ? "password" : "text",
        placeholder: present[field.key] ? "••••••••  (stored)" : field.placeholder,
        autocomplete: "off",
        spellcheck: "false",
        style: "flex:1 1 auto;min-width:0",
      }) as HTMLInputElement;
      const saveBtn = h("button", { text: "Save" });
      const dotEl = statusDot(present[field.key] ?? false);
      saveBtn.addEventListener("click", async () => {
        const value = input.value.trim();
        try {
          await Bridge.secretSet(field.key, value);
          present[field.key] = value.length > 0;
          input.value = "";
          input.placeholder = value ? "••••••••  (stored)" : field.placeholder;
          dotEl.style.background = value ? "#22c55e" : "#f4505e";
        } catch {
          dotEl.style.background = "#f5a524";
        }
      });
      rows.append(
        h("div", { class: "row" },
          h("label", { style: "min-width:104px", text: field.label }),
          input, saveBtn, dotEl,
        ),
      );
    }

    list.append(
      h("div", { style: "display:flex;gap:12px;align-items:flex-start" },
        h("div", { style: "display:flex;align-items:center;gap:8px;min-width:132px;padding-top:4px" },
          sw,
          h("i", { class: "dot", style: `background:${def.color}` }),
          h("span", { style: "font-size:12.5px", text: def.name }),
        ),
        rows,
      ),
    );
  }

  updateNote();
  return h("section", {}, h("h2", {}, h("span", { text: "Integrations" })), note, list);
}

// ── Music section ─────────────────────────────────────────────────────────────

function musicSection(): HTMLElement {
  const visualizer = h("select", {}) as HTMLSelectElement;
  visualizer.append(
    h("option", { value: "realtime", text: "Real-time (cava)" }),
    h("option", { value: "wave", text: "Wave" }),
    h("option", { value: "beat", text: "Beat" }),
    h("option", { value: "off", text: "Off" }),
  );
  visualizer.value = settings.musicVisualizer;
  visualizer.addEventListener("change", () => {
    settings.musicVisualizer = visualizer.value as Settings["musicVisualizer"];
    void save();
  });

  const scroll = h("select", {}) as HTMLSelectElement;
  scroll.append(
    h("option", { value: "track", text: "Change track" }),
    h("option", { value: "volume", text: "Change volume" }),
  );
  scroll.value = settings.musicScroll;
  scroll.addEventListener("change", () => {
    settings.musicScroll = scroll.value as Settings["musicScroll"];
    void save();
  });

  const language = h("select", {}) as HTMLSelectElement;
  language.append(
    h("option", { value: "any", text: "Closest in length" }),
    h("option", { value: "latin", text: "Latin letters" }),
    h("option", { value: "original", text: "Original script" }),
  );
  language.value = settings.musicLyricsLanguage;
  language.addEventListener("change", () => {
    settings.musicLyricsLanguage = language.value as Settings["musicLyricsLanguage"];
    void save();
  });

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Music" })),
    h("div", { class: "hint", text:
      "Turn on the Music pill above (it takes one of the four slots). It follows whichever player is playing, " +
      "tints the island with the cover's colour and lets you play, skip and seek from it." }),
    h("div", { class: "row" },
      h("label", { text: "Lyrics and web covers" }),
      toggle(settings.musicOnline, (v) => { settings.musicOnline = v; void save(); }),
      h("span", { class: "hint", text:
        "Fetch synchronised lyrics from lrclib.net and covers the player gives as a web address. " +
        "Off by default: both are network requests. A cover that is a local file is always used." }),
    ),
    h("div", { class: "row" },
      h("label", { text: "Keep after the player closes" }),
      toggle(settings.musicKeep, (v) => { settings.musicKeep = v; void save(); }),
      h("span", { class: "hint", text:
        "The pill stays on the last track, so the minimised island does not hide. Off: it goes with the player." }),
    ),
    h("div", { class: "row" },
      h("label", { text: "Open on hover" }),
      toggle(settings.openOnHover, (v) => { settings.openOnHover = v; void save(); }),
      h("span", { class: "hint", text:
        "The minimised island opens when the pointer rests on it and folds back right after it leaves, " +
        "from any view. Right-click and double-click no longer open it." }),
    ),
    h("div", { class: "row" },
      h("label", { text: "Hover time" }),
      ...hoverDelayControls(),
    ),
    h("div", { class: "row" },
      h("label", { text: "Wheel on the pill" }),
      scroll,
      h("span", { class: "hint", text: "over the minimised island (needs a player with a volume control for Volume)" }),
    ),
    h("div", { class: "row" },
      h("label", { text: "Lyrics script" }),
      language,
      h("span", { class: "hint", text: "when a song has several versions (original, romanised…)" }),
    ),
    h("div", { class: "row" },
      h("label", { text: "Visualizer" }),
      visualizer,
      h("span", { class: "hint", text:
        "Real-time follows the sound with cava (only its levels, only while you can see the bars; " +
        "animated bars are used if cava is not installed). Wave and Beat are animations." }),
    ),
  );
}

// ── General section ───────────────────────────────────────────────────────────

/** The slider and read-out of how long the pointer must rest on the island to open it. */
function hoverDelayControls(): HTMLElement[] {
  const value = h("span", { class: "hint", text: "" });
  const slider = h("input", {
    type: "range", min: "0.3", max: "3", step: "0.1",
    value: String(settings.hoverOpenDelay ?? 0.9), style: "flex:1 1 auto",
  }) as HTMLInputElement;
  const show = () => { value.textContent = `${Number(slider.value).toFixed(1)} s`; };
  show();
  slider.addEventListener("input", show);
  slider.addEventListener("change", () => {
    settings.hoverOpenDelay = Number(slider.value);
    void save();
  });
  return [slider, value];
}

/** The "finished" sound: a built-in, none, or a file of your own, with a preview. */
function finishSoundRow(): HTMLElement {
  const names = [
    "finish", "approve", "proud", "pop", "wink", "greet", "tick", "love", "attach", "send", "blip",
    "peek", "open", "close", "hover", "slap", "annoyed", "dizzy", "work", "error", "approval",
    "question", "gulp", "think", "search", "rate", "sleep", "yawn",
  ];
  const choice = h("select", {}) as HTMLSelectElement;
  choice.append(
    h("option", { value: "finish", text: "Default" }),
    h("option", { value: "none", text: "No sound" }),
    h("option", { value: "file", text: "My own file…" }),
  );
  for (const n of names.filter((x) => x !== "finish")) choice.append(h("option", { value: n, text: n }));
  const known = ["none", "file", ...names];
  choice.value = known.includes(settings.finishSound) ? settings.finishSound : "finish";

  const file = h("input", {
    type: "text", placeholder: "/path/to/sound.wav (wav, ogg, mp3, flac)", spellcheck: "false",
    value: settings.finishSoundFile ?? "", style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  const note = h("span", { class: "hint", text: "" });
  const play = h("button", { text: "▶ Preview" });
  const syncVisibility = () => {
    file.style.display = choice.value === "file" ? "" : "none";
    play.style.display = choice.value === "none" ? "none" : "";
  };
  syncVisibility();

  let preview: HTMLAudioElement | null = null;
  async function previewSound() {
    note.textContent = "";
    try {
      preview?.pause();
      let url: string;
      if (choice.value === "file") {
        const b64 = await Bridge.readSound(file.value.trim());
        url = URL.createObjectURL(new Blob([Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))]));
      } else {
        url = `/sounds/${choice.value}.wav`;
      }
      preview = new Audio(url);
      // The island plays through a gain of this value; the same level here.
      preview.volume = Math.max(0.05, Math.min(1, settings.soundVolume * 2));
      await preview.play();
    } catch (err) {
      note.textContent = String(err).replace(/^Error:\s*/, "");
    }
  }
  play.addEventListener("click", () => void previewSound());
  choice.addEventListener("change", () => {
    syncVisibility();
    settings.finishSound = choice.value;
    void save();
    if (choice.value !== "file") void previewSound();
  });
  file.addEventListener("change", () => {
    settings.finishSoundFile = file.value.trim();
    void save();
    if (choice.value === "file") void previewSound();
  });

  return h("div", { class: "row" }, h("label", { text: "Finished sound" }), choice, file, play, note);
}

/** The Hub's sources: the server it reads over ssh. */
function hubSection(): HTMLElement {
  const host = h("input", {
    type: "text", placeholder: "server  (a name from ~/.ssh/config, or user@host)", spellcheck: "false",
    value: settings.serverHost ?? "", style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  host.addEventListener("change", () => {
    settings.serverHost = host.value.trim();
    void save();
  });
  const services = h("input", {
    type: "text", placeholder: "hermes-gateway,ollama,docker", spellcheck: "false",
    value: settings.serverServices ?? "", style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  services.addEventListener("change", () => {
    settings.serverServices = services.value.trim();
    void save();
  });
  const field = (value: string, placeholder: string, apply: (v: string) => void, width = "flex:1 1 auto;min-width:0") => {
    const input = h("input", { type: "text", placeholder, spellcheck: "false", value, style: width }) as HTMLInputElement;
    input.addEventListener("change", () => {
      apply(input.value.trim());
      void save();
    });
    return input;
  };
  const driveRemote = field(settings.driveRemote ?? "gdrive", "rclone remote (gdrive)", (v) => (settings.driveRemote = v || "gdrive"), "width:120px");
  const driveFolder = field(settings.driveFolder ?? "Coucou", "Drive folder (Coucou)", (v) => (settings.driveFolder = v || "Coucou"));
  const driveAccount = field(settings.driveAccount ?? "", "Google account (you@gmail.com)", (v) => (settings.driveAccount = v.trim()));
  const weatherCity = field(settings.weatherCity ?? "", "City (Zapopan)", (v) => (settings.weatherCity = v));
  const weatherCity2 = field(settings.weatherCity2 ?? "", "Second city (optional)", (v) => (settings.weatherCity2 = v));
  const schoolFolder = field(settings.schoolFolder ?? "Por clasificar", "Por clasificar", (v) => (settings.schoolFolder = v || "Por clasificar"));
  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "Hub" })),
    h("div", { class: "hint", text:
      "The Hub (the fourth tab of the island) shows this computer's vitals, your server's, a Pomodoro timer, " +
      "notes, your calendar, the weather, a file shelf and quick switches. It reads nothing while it is closed." }),
    h("div", { class: "row" }, h("label", { text: "Server" }), host,
      h("span", { class: "hint", text: "read over ssh with your key; nothing is installed there" })),
    h("div", { class: "row" }, h("label", { text: "Share with Jinx" }),
      toggle(settings.jinxShare !== false, (v) => { settings.jinxShare = v; void save(); }),
      h("span", { class: "hint", text:
        "Your notes, calendar and timer are copied to ~/.hermes/state/coucou/ on the server so Jinx can read them. " +
        "Nothing else on the server is touched." })),
    h("div", { class: "row" }, h("label", { text: "Notifications" }),
      toggle(settings.notificationPeek === true, (v) => { settings.notificationPeek = v; void save(); }),
      h("span", { class: "hint", text:
        "Show other programs' notifications in the island. It reads what they say (messages, mail), so it is off by " +
        "until you turn it on; nothing is stored or logged. It cannot hide the desktop's own banner." })),
    h("div", { class: "row" }, h("label", { text: "Phone" }),
      toggle(settings.remoteEnabled === true, (v) => { settings.remoteEnabled = v; void save(); }),
      h("span", { class: "hint", text:
        "Let your phone follow Claude Code and answer its permission requests and questions. It listens to a private " +
        "socket that only an ssh key restricted to `coucou-hook --remote` can reach; nothing is opened on the network." })),
    h("div", { class: "row" }, h("label", { text: "Weather" }),
      toggle(settings.weatherOn === true, (v) => { settings.weatherOn = v; void save(); }), weatherCity, weatherCity2,
      h("span", { class: "hint", text: "asks open-meteo.com for these cities, only while the Weather tab is open" })),
    h("div", { class: "row" }, h("label", { text: "Uploads" }), driveAccount, driveFolder, driveRemote,
      h("span", { class: "hint", text: "Google Drive goes through GNOME Online Accounts (the account above, the folder under My Drive); the rclone remote is the fallback. School OneDrive uses Jinx's sign-in." })),
    h("div", { class: "row" }, h("label", { text: "School folder" }), schoolFolder,
      h("span", { class: "hint", text: "e.g. Por clasificar, or Materias/BIOLOGIA I/Actividades" })),
    h("div", { class: "row" }, h("label", { text: "Services" }), services,
      h("span", { class: "hint", text: "systemd units shown on the Server tab" })),
  );
}

function generalSection(): HTMLElement {
  const volume = h("input", {
    type: "range", min: "0", max: "0.2", step: "0.005",
    value: String(settings.soundVolume),
  }) as HTMLInputElement;
  volume.addEventListener("input", () => {
    settings.soundVolume = Number(volume.value);
    void save();
  });

  const autoClose = h("input", {
    type: "number", min: "5", max: "120", step: "1",
    value: String(Math.round(settings.autoCloseInterval)),
    style: "width:72px",
  }) as HTMLInputElement;
  autoClose.addEventListener("change", () => {
    settings.autoCloseInterval = Math.max(5, Math.min(120, Number(autoClose.value) || 15));
    autoClose.value = String(settings.autoCloseInterval);
    void save();
  });

  // Where the island sits across the top; a window tiled beside the middle has its
  // buttons under a centred one.
  const offsetValue = h("span", { class: "hint", text: "" });
  const offset = h("input", {
    type: "range", min: "-900", max: "900", step: "10",
    value: String(settings.islandOffsetX ?? 0), style: "flex:1 1 auto",
  }) as HTMLInputElement;
  const showOffset = () => {
    const v = Number(offset.value);
    offsetValue.textContent = v === 0 ? "centred" : `${Math.abs(v)} px ${v > 0 ? "right" : "left"}`;
  };
  showOffset();
  offset.addEventListener("input", showOffset);
  offset.addEventListener("change", () => {
    settings.islandOffsetX = Number(offset.value);
    void save();
  });

  const screen = h("select", {}) as HTMLSelectElement;
  screen.append(
    h("option", { value: "primary", text: "Main display" }),
    h("option", { value: "cursor", text: "Display under the cursor" }),
  );
  screen.value = settings.screen;
  screen.addEventListener("change", () => {
    settings.screen = screen.value as Settings["screen"];
    void save();
  });

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: "General" })),
    h("div", { class: "row" },
      h("label", { text: "Sound" }),
      toggle(settings.soundEnabled, (v) => { settings.soundEnabled = v; void save(); }),
      volume,
    ),
    finishSoundRow(),
    h("div", { class: "row" },
      h("label", { text: "Auto-close" }),
      autoClose,
      h("span", { class: "hint", text: "seconds after you leave the island" }),
    ),
    h("div", { class: "row" },
      h("label", { text: "Island lives on" }),
      screen,
    ),
    h("div", { class: "row" },
      h("label", { text: "Step aside" }),
      toggle(settings.dodgeWindows, (v) => { settings.dodgeWindows = v; void save(); }),
      h("span", { class: "hint", text:
        "When you stop the pointer on the buttons of a window the island covers, it moves out of the way " +
        "(and goes back after). Resting on the island itself keeps it where it is." }),
    ),
    h("div", { class: "row" },
      h("label", { text: "Island position" }),
      offset,
      offsetValue,
    ),
    h("div", { class: "row" },
      h("label", { text: "Launch at startup" }),
      toggle(settings.autostart, (v) => { settings.autostart = v; void save(); }),
    ),
  );
}

// ── Boot ──────────────────────────────────────────────────────────────────────

async function main() {
  const boot = await Bridge.boot();
  if (boot) {
    settings = { ...settings, ...boot.settings };
    version = boot.version;
  }
  const status = (await Bridge.hooksStatus()) ?? {
    installed: false, askInstalled: false, settingsPath: "", hookPath: "", hookReady: false,
  };

  const hasKey = (await Bridge.secretPresent("anthropic-api-key")) ?? false;

  const keys = [
    "stripe-api-key", "github-token", "vercel-token",
    "n8n-url", "n8n-api-key", "resend-api-key", "notion-api-key", "calcom-api-key",
  ];
  const present: Record<string, boolean> = {};
  for (const k of keys) present[k] = (await Bridge.secretPresent(k)) ?? false;

  clear(root);
  root.append(
    h("h1", {}, h("span", { text: "Coucou" }), h("span", { class: "version", text: version })),
    claudeSection(status),
    apiSection(hasKey),
    integrationsSection(present),
    musicSection(),
    hubSection(),
    generalSection(),
    h("div", {
      class: "hint",
      text: "No telemetry. Network requests only go to the services you configure yourself.",
    }),
  );

  void onEvent<Settings>("settings-changed", (s) => {
    settings = { ...settings, ...s };
  });
}

void main();
