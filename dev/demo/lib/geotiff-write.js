// Minimal GeoTIFF writer: one uncompressed float32 band in a single strip,
// georeferenced in EPSG:4326 with a north-up pixel grid. Used to hand
// risico-2023 a domain mask and static layers built in the browser.

const SHORT = 3;
const LONG = 4;
const DOUBLE = 12;
const ASCII = 2;

/**
 * @param {{ width: number, height: number, originLon: number, originLat: number,
 *   pixelWidth: number, pixelHeight: number, values: ArrayLike<number>, nodata?: number }} raster
 *   `originLon`/`originLat` are the outer north-west corner; rows run north to south.
 * @returns {Uint8Array}
 */
export function writeGeoTiff({ width, height, originLon, originLat, pixelWidth, pixelHeight, values, nodata = -9999 }) {
  if (values.length !== width * height) {
    throw new Error(`GeoTIFF has ${values.length} values, expected ${width * height}`);
  }
  const nodataText = `${nodata}\0`;
  const geoKeys = [
    1, 1, 0, 3,
    1024, 0, 1, 2, // GTModelType: geographic
    1025, 0, 1, 1, // GTRasterType: pixel is area
    2048, 0, 1, 4326, // GeographicType: WGS 84
  ];
  const entries = [
    { tag: 256, type: LONG, values: [width] },
    { tag: 257, type: LONG, values: [height] },
    { tag: 258, type: SHORT, values: [32] },
    { tag: 259, type: SHORT, values: [1] },
    { tag: 262, type: SHORT, values: [1] },
    { tag: 273, type: LONG, values: [0] }, // strip offset, patched below
    { tag: 277, type: SHORT, values: [1] },
    { tag: 278, type: LONG, values: [height] },
    { tag: 279, type: LONG, values: [width * height * 4] },
    { tag: 284, type: SHORT, values: [1] },
    { tag: 339, type: SHORT, values: [3] },
    { tag: 33550, type: DOUBLE, values: [pixelWidth, pixelHeight, 0] },
    { tag: 33922, type: DOUBLE, values: [0, 0, 0, originLon, originLat, 0] },
    { tag: 34735, type: SHORT, values: geoKeys },
    { tag: 42113, type: ASCII, values: [...nodataText].map((c) => c.charCodeAt(0)) },
  ];
  const sizeOf = { [SHORT]: 2, [LONG]: 4, [DOUBLE]: 8, [ASCII]: 1 };

  const ifdOffset = 8;
  const ifdSize = 2 + entries.length * 12 + 4;
  let extra = ifdOffset + ifdSize;
  for (const entry of entries) {
    const bytes = entry.values.length * sizeOf[entry.type];
    if (bytes > 4) {
      entry.offset = extra;
      extra += bytes + (bytes & 1);
    }
  }
  const dataOffset = (extra + 3) & ~3;
  entries.find((entry) => entry.tag === 273).values[0] = dataOffset;

  const out = new Uint8Array(dataOffset + width * height * 4);
  const view = new DataView(out.buffer);
  out.set([0x49, 0x49, 42, 0]);
  view.setUint32(4, ifdOffset, true);
  view.setUint16(ifdOffset, entries.length, true);

  const put = (type, at, value) => {
    switch (type) {
      case SHORT: view.setUint16(at, value, true); break;
      case LONG: view.setUint32(at, value, true); break;
      case DOUBLE: view.setFloat64(at, value, true); break;
      case ASCII: view.setUint8(at, value); break;
    }
  };
  entries.forEach((entry, index) => {
    const at = ifdOffset + 2 + index * 12;
    view.setUint16(at, entry.tag, true);
    view.setUint16(at + 2, entry.type, true);
    view.setUint32(at + 4, entry.values.length, true);
    if (entry.offset !== undefined) view.setUint32(at + 8, entry.offset, true);
    const base = entry.offset ?? at + 8;
    entry.values.forEach((value, i) => put(entry.type, base + i * sizeOf[entry.type], value));
  });
  view.setUint32(ifdOffset + 2 + entries.length * 12, 0, true);

  for (let i = 0; i < values.length; i++) {
    view.setFloat32(dataOffset + i * 4, values[i], true);
  }
  return out;
}
