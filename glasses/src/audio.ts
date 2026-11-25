// The glasses' microphone arrives as many small PCM frames (16 kHz, mono, s16le). Collect them while the button is
// held; the bridge refuses more than 60 s, so we stop collecting there.

export const MAX_BYTES = 16000 * 2 * 60;

export function createRecorder() {
  let chunks: Uint8Array[] = [];
  let size = 0;
  return {
    push(data: Uint8Array): void {
      if (size >= MAX_BYTES) return;
      const room = MAX_BYTES - size;
      const piece = data.length > room ? data.subarray(0, room) : data;
      chunks.push(piece);
      size += piece.length;
    },
    get bytes(): number { return size; },
    /** All the audio so far, as one buffer (an even number of bytes), and starts over. */
    take(): Uint8Array {
      const out = new Uint8Array(size - (size % 2));
      let at = 0;
      for (const c of chunks) {
        const n = Math.min(c.length, out.length - at);
        if (n <= 0) break;
        out.set(c.subarray(0, n), at);
        at += n;
      }
      chunks = [];
      size = 0;
      return out;
    },
    clear(): void { chunks = []; size = 0; },
  };
}
