// Writes app.json for `evenhub pack`. The bridge's origin must be whitelisted exactly (no wildcards) and it is
// personal, so the manifest is generated from JINX_G2_ORIGIN instead of being committed.
//   JINX_G2_ORIGIN=https://jinx.tu-red.ts.net npm run pack
import { writeFileSync } from "node:fs";

export function manifest(origin) {
  let url;
  try {
    url = new URL(origin);
  } catch {
    throw new Error("JINX_G2_ORIGIN no es una dirección: pon p. ej. https://jinx.tu-red.ts.net");
  }
  if (url.protocol !== "https:") throw new Error("JINX_G2_ORIGIN debe ser https:// (Tailscale Serve lo da)");
  return {
    package_id: "com.alan.jinx",
    edition: "202601",
    name: "Jinx",
    version: "0.1.0",
    min_app_version: "2.2.10",
    min_sdk_version: "0.0.16",
    entrypoint: "index.html",
    permissions: [
      { name: "g2-microphone", desc: "Hablar con Jinx: la voz se manda a tu propio servidor." },
      { name: "network", desc: "Hablar con tu servidor de Jinx.", whitelist: [url.origin] },
    ],
    supported_languages: ["es", "en"],
  };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const m = manifest(process.env.JINX_G2_ORIGIN ?? "");
  writeFileSync(new URL("../app.json", import.meta.url), JSON.stringify(m, null, 2) + "\n");
  console.log("app.json escrito para", m.permissions[1].whitelist[0]);
}
