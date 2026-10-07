// Generates src-tauri/icons/{app-icon,tray,tray-muted}.png with no dependencies.
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";

const crcTable = new Uint32Array(256).map((_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (buf) => {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}
function png(size, pixel) {
  const stride = size * 4 + 1;
  const raw = Buffer.alloc(stride * size);
  for (let y = 0; y < size; y++) {
    raw[y * stride] = 0;
    for (let x = 0; x < size; x++) {
      const [r, g, b, a] = pixel((x + 0.5) / size, (y + 0.5) / size, size);
      const o = y * stride + 1 + x * 4;
      raw[o] = r; raw[o + 1] = g; raw[o + 2] = b; raw[o + 3] = a;
    }
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; ihdr[9] = 6; ihdr[10] = 0; ihdr[11] = 0; ihdr[12] = 0;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}
const clamp = (v) => Math.max(0, Math.min(1, v));
const mix = (a, b, t) => a.map((av, i) => av + (b[i] - av) * t);
const ACCENT = [139, 108, 255];
const LIGHT = [236, 235, 243];
const BASE = [24, 24, 31];
const RED = [255, 106, 92];

/** Rounded-square badge with a centre dot and two "resonance" rings. */
function glyph({ muted }) {
  return (u, v, s) => {
    const aa = 1.5 / s;
    const r = 0.22;
    const qx = Math.abs(u - 0.5) - (0.5 - r);
    const qy = Math.abs(v - 0.5) - (0.5 - r);
    const box = Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) + Math.min(Math.max(qx, qy), 0) - r;
    let alpha = clamp(0.5 - box / aa);
    let col = BASE;
    const d = Math.hypot(u - 0.5, v - 0.5);
    const dot = clamp(0.5 - (d - 0.09) / aa);
    col = mix(col, ACCENT, dot);
    const ring1 = clamp(0.5 - (Math.abs(d - 0.21) - 0.025) / aa);
    col = mix(col, ACCENT, ring1);
    const ring2 = clamp(0.5 - (Math.abs(d - 0.33) - 0.02) / aa);
    col = mix(col, LIGHT, ring2);
    if (muted) {
      const md = Math.hypot(u - 0.76, v - 0.76);
      const k = clamp(0.5 - (md - 0.22) / aa);
      col = mix(col, RED, k);
      alpha = Math.max(alpha, k);
    }
    return [...col.map(Math.round), Math.round(alpha * 255)];
  };
}

mkdirSync("src-tauri/icons", { recursive: true });
writeFileSync("src-tauri/icons/app-icon.png", png(1024, glyph({ muted: false })));
writeFileSync("src-tauri/icons/tray.png", png(64, glyph({ muted: false })));
writeFileSync("src-tauri/icons/tray-muted.png", png(64, glyph({ muted: true })));
console.log("icons written");
