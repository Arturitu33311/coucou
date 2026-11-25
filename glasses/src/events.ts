// The glasses' raw events as the app's own inputs. The numeric values are the SDK's OsEventTypeList (a test in
// main.ts's typecheck keeps them honest); this file imports nothing so tests can run it directly.

export const EV = {
  CLICK: 0,
  SCROLL_TOP: 1,
  SCROLL_BOTTOM: 2,
  DOUBLE_CLICK: 3,
  FOREGROUND_ENTER: 4,
  FOREGROUND_EXIT: 5,
  ABNORMAL_EXIT: 6,
  SYSTEM_EXIT: 7,
  LONG_PRESS: 9,
  LONG_PRESS_RELEASE: 10,
} as const;

export interface RawEnvelope { eventType?: number }
export interface RawEvent {
  sysEvent?: RawEnvelope;
  textEvent?: RawEnvelope;
  listEvent?: RawEnvelope;
  audioEvent?: { audioPcm?: Uint8Array | number[] };
}

export type Input =
  | { t: "tap" }
  | { t: "double" }
  | { t: "up" }
  | { t: "down" }
  | { t: "pressStart" }
  | { t: "pressEnd" }
  | { t: "foreground" }
  | { t: "background" }
  | { t: "exit" }
  | { t: "pcm"; data: Uint8Array };

/**
 * CLICK is 0 and protobuf leaves zero out, so a tap arrives as an envelope whose eventType is undefined: the
 * default is applied only when the envelope exists (an event with no sysEvent is not a tap).
 */
function typeOf(env?: RawEnvelope): number | null {
  if (!env) return null;
  return env.eventType ?? EV.CLICK;
}

/** One raw event as zero or more inputs. Double-tap wins over everything else in the same event. */
export function decode(event: RawEvent): Input[] {
  const out: Input[] = [];
  const pcm = event.audioEvent?.audioPcm;
  if (pcm && pcm.length) out.push({ t: "pcm", data: pcm instanceof Uint8Array ? pcm : Uint8Array.from(pcm) });
  const types = [typeOf(event.sysEvent), typeOf(event.textEvent), typeOf(event.listEvent)]
    .filter((x): x is number => x !== null);
  if (types.includes(EV.DOUBLE_CLICK)) return [{ t: "double" }, ...out];
  const first = types[0];
  switch (first) {
    case EV.CLICK: out.push({ t: "tap" }); break;
    case EV.SCROLL_TOP: out.push({ t: "up" }); break;
    case EV.SCROLL_BOTTOM: out.push({ t: "down" }); break;
    case EV.LONG_PRESS: out.push({ t: "pressStart" }); break;
    case EV.LONG_PRESS_RELEASE: out.push({ t: "pressEnd" }); break;
    case EV.FOREGROUND_ENTER: out.push({ t: "foreground" }); break;
    case EV.FOREGROUND_EXIT: out.push({ t: "background" }); break;
    case EV.ABNORMAL_EXIT:
    case EV.SYSTEM_EXIT: out.push({ t: "exit" }); break;
    default: break;
  }
  return out;
}
