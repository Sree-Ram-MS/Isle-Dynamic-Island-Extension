// Generates assets/icon.png (the mascot eyes on a dark rounded square)
// with zero dependencies, so the PNG never has to live in git as a binary.
// Runs automatically before `npm start` and `npm run dist`.
const fs = require('fs');
const path = require('path');
const zlib = require('zlib');

const S = 64;
const px = new Uint8Array(S * S * 4);

function set(x, y, r, g, b, a) {
    if (x < 0 || y < 0 || x >= S || y >= S) return;
    const i = (y * S + x) * 4;
    px[i] = r; px[i + 1] = g; px[i + 2] = b; px[i + 3] = a;
}
function disc(cx, cy, rad, r, g, b) {
    for (let y = Math.floor(cy - rad); y <= cy + rad; y++)
        for (let x = Math.floor(cx - rad); x <= cx + rad; x++)
            if ((x - cx) ** 2 + (y - cy) ** 2 <= rad * rad) set(x, y, r, g, b, 255);
}

// dark rounded-square background
const R = 14;
for (let y = 0; y < S; y++) {
    for (let x = 0; x < S; x++) {
        const cx = Math.max(R - x, 0, x - (S - 1 - R));
        const cy = Math.max(R - y, 0, y - (S - 1 - R));
        if (cx * cx + cy * cy <= R * R) set(x, y, 14, 14, 17, 255);
    }
}
// two eyes (white + pupil), the Nusa-style mascot
for (const cx of [24, 40]) {
    disc(cx, 30, 7, 245, 245, 248);
    disc(cx, 31, 3.4, 16, 16, 20);
}

// --- minimal PNG encoder ---
const crcTable = [];
for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crcTable[n] = c >>> 0;
}
function crc32(buf) {
    let c = 0xffffffff;
    for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
}
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(S, 0);
ihdr.writeUInt32BE(S, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
const raw = Buffer.alloc(S * (S * 4 + 1));
for (let y = 0; y < S; y++) {
    raw[y * (S * 4 + 1)] = 0; // no filter
    Buffer.from(px.buffer, y * S * 4, S * 4).copy(raw, y * (S * 4 + 1) + 1);
}
const png = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', zlib.deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
]);
fs.mkdirSync(path.join(__dirname, 'assets'), {recursive: true});
fs.writeFileSync(path.join(__dirname, 'assets', 'icon.png'), png);
console.log('assets/icon.png written', png.length, 'bytes');
