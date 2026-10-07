import fs from 'node:fs';
import zlib from 'node:zlib';

function dosDateTime(date) {
  const time =
    (date.getHours() << 11) |
    (date.getMinutes() << 5) |
    Math.floor(date.getSeconds() / 2);
  const day =
    ((date.getFullYear() - 1980) << 9) |
    ((date.getMonth() + 1) << 5) |
    date.getDate();
  return { time, day };
}

/** Writes a deflated ZIP with forward-slash names; entries are [name, bytes]. */
export function writeZip(file, entries, date = new Date()) {
  const { time, day } = dosDateTime(date);
  const locals = [];
  const central = [];
  let offset = 0;
  for (const [name, bytes] of entries) {
    if (name.includes('\\')) throw new Error(`${name}: use forward slashes`);
    const nameBytes = Buffer.from(name, 'utf8');
    const data = zlib.deflateRawSync(bytes, { level: 9 });
    const crc = zlib.crc32(bytes);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt16LE(0x0800, 6);
    header.writeUInt16LE(8, 8);
    header.writeUInt16LE(time, 10);
    header.writeUInt16LE(day, 12);
    header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(data.length, 18);
    header.writeUInt32LE(bytes.length, 22);
    header.writeUInt16LE(nameBytes.length, 26);
    const record = Buffer.alloc(46);
    record.writeUInt32LE(0x02014b50, 0);
    record.writeUInt16LE(20, 4);
    record.writeUInt16LE(20, 6);
    record.writeUInt16LE(0x0800, 8);
    record.writeUInt16LE(8, 10);
    record.writeUInt16LE(time, 12);
    record.writeUInt16LE(day, 14);
    record.writeUInt32LE(crc, 16);
    record.writeUInt32LE(data.length, 20);
    record.writeUInt32LE(bytes.length, 24);
    record.writeUInt16LE(nameBytes.length, 28);
    record.writeUInt32LE(offset, 42);
    locals.push(header, nameBytes, data);
    central.push(record, nameBytes);
    offset += header.length + nameBytes.length + data.length;
  }
  const directory = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  fs.writeFileSync(file, Buffer.concat([...locals, directory, end]), {
    flag: 'wx',
  });
}

/** Reads every entry of a ZIP written by writeZip, checking CRCs and sizes. */
export function readZip(file) {
  const zip = fs.readFileSync(file);
  const end = zip.length - 22;
  if (zip.readUInt32LE(end) !== 0x06054b50) throw new Error('ZIP end missing');
  const count = zip.readUInt16LE(end + 10);
  let at = zip.readUInt32LE(end + 16);
  const entries = new Map();
  for (let index = 0; index < count; index += 1) {
    if (zip.readUInt32LE(at) !== 0x02014b50) throw new Error('ZIP directory');
    const crc = zip.readUInt32LE(at + 16);
    const size = zip.readUInt32LE(at + 24);
    const nameLength = zip.readUInt16LE(at + 28);
    const extra = zip.readUInt16LE(at + 30) + zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const name = zip.toString('utf8', at + 46, at + 46 + nameLength);
    const start =
      local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
    const compressed = zip.subarray(start, start + zip.readUInt32LE(at + 20));
    const bytes = zlib.inflateRawSync(compressed);
    if (bytes.length !== size || zlib.crc32(bytes) !== crc) {
      throw new Error(`${name}: ZIP entry is corrupt`);
    }
    entries.set(name, bytes);
    at += 46 + nameLength + extra;
  }
  return entries;
}
