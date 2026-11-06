// What a command reaches, in a word, for the permission card (AgentBar's "risk labels").
// Read-only advice on top of the command text the card already shows: it never allows, denies
// or changes anything. A label is a hint to read the line twice, not a verdict; a command with
// none is not thereby safe.

export interface RiskTag {
  /** Short text on the chip. */
  label: string;
  /** "high" is red (can destroy or run unseen code), "mid" is amber (reaches beyond the folder). */
  level: "high" | "mid";
}

const RULES: { re: RegExp; tag: RiskTag }[] = [
  // Deleting: rm with -r/-f in any order, or find … -delete.
  { re: /(^|[\s;&|(])rm\s+(-[a-zA-Z]*[rRfF][a-zA-Z]*|--recursive|--force)\b/, tag: { label: "deletes files", level: "high" } },
  { re: /\bfind\b[^|;&]*\s-delete\b/, tag: { label: "deletes files", level: "high" } },
  // Code from the network run unseen: curl/wget piped into a shell or an interpreter.
  { re: /\b(curl|wget)\b[^|;&]*\|\s*(sudo\s+)?(ba|z|da|k)?sh\b/, tag: { label: "runs downloaded code", level: "high" } },
  { re: /\b(curl|wget)\b[^|;&]*\|\s*(sudo\s+)?(python3?|node|perl|ruby)\b/, tag: { label: "runs downloaded code", level: "high" } },
  { re: /\b(ba|z|da)?sh\s+-c\s+["']?\$\((curl|wget)\b/, tag: { label: "runs downloaded code", level: "high" } },
  { re: /(^|[\s;&|(])sudo\b/, tag: { label: "as root", level: "high" } },
  { re: /\b(mkfs(\.\w+)?|fdisk|parted|wipefs)\b/, tag: { label: "formats a disk", level: "high" } },
  { re: /\bdd\s+[^|;&]*\bof=\/dev\//, tag: { label: "writes to a disk", level: "high" } },
  { re: />\s*\/dev\/(sd|nvme|mmcblk)/, tag: { label: "writes to a disk", level: "high" } },
  { re: /\bchmod\s+(-R\s+)?0?777\b/, tag: { label: "opens permissions", level: "high" } },
  // History rewriting and publishing.
  { re: /\bgit\s+push\b[^|;&]*(--force\b|-f\b|--force-with-lease\b)/, tag: { label: "force-pushes", level: "high" } },
  { re: /\bgit\s+(reset\s+--hard|clean\s+-[a-zA-Z]*f)/, tag: { label: "discards changes", level: "high" } },
  { re: /\b(npm|pnpm|yarn)\s+publish\b|\bcargo\s+publish\b|\btwine\s+upload\b/, tag: { label: "publishes a package", level: "high" } },
  { re: /\b(drop|truncate)\s+(table|database)\b/i, tag: { label: "drops data", level: "high" } },
  // Reaching out: not destructive, but the command leaves the machine.
  { re: /\b(curl|wget|scp|rsync|ssh|sftp|nc|ncat)\b/, tag: { label: "uses the network", level: "mid" } },
  { re: /\b(pip3?|npm|pnpm|yarn|cargo|apt(-get)?|gem)\s+(install|add)\b/, tag: { label: "installs software", level: "mid" } },
  { re: /(^|[\s;&|(])(kill|pkill|killall)\b/, tag: { label: "kills processes", level: "mid" } },
  { re: /\bgit\s+push\b/, tag: { label: "pushes to a remote", level: "mid" } },
];

/** The labels for a command (or other target text), at most four, the red ones first. */
export function riskTags(command: string): RiskTag[] {
  const text = (command ?? "").slice(0, 4000);
  const seen = new Set<string>();
  const out: RiskTag[] = [];
  for (const { re, tag } of RULES) {
    if (!seen.has(tag.label) && re.test(text)) {
      seen.add(tag.label);
      out.push(tag);
    }
  }
  out.sort((a, b) => (a.level === b.level ? 0 : a.level === "high" ? -1 : 1));
  return out.slice(0, 4);
}
