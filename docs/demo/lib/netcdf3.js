// Minimal NetCDF classic (CDF-1) writer: fixed-size dimensions, global and
// variable attributes, byte/char/short/int/float/double variables. Enough to
// hand risico-2023 its weather inputs from the browser.

const NC_DIMENSION = 10;
const NC_VARIABLE = 11;
const NC_ATTRIBUTE = 12;

const TYPES = {
  byte: { code: 1, size: 1, put: (v, o, x) => v.setInt8(o, x) },
  char: { code: 2, size: 1, put: (v, o, x) => v.setUint8(o, x) },
  short: { code: 3, size: 2, put: (v, o, x) => v.setInt16(o, x) },
  int: { code: 4, size: 4, put: (v, o, x) => v.setInt32(o, x) },
  float: { code: 5, size: 4, put: (v, o, x) => v.setFloat32(o, x) },
  double: { code: 6, size: 8, put: (v, o, x) => v.setFloat64(o, x) },
};

const pad4 = (n) => (n + 3) & ~3;
const encoder = new TextEncoder();

/**
 * @param {{
 *   dimensions: Record<string, number>,
 *   attributes?: Record<string, string | number>,
 *   variables: Array<{ name: string, type: keyof TYPES, dimensions: string[],
 *     attributes?: Record<string, string | number>, data: ArrayLike<number> }>
 * }} spec
 * @returns {Uint8Array}
 */
export function writeNetcdf3(spec) {
  const dimNames = Object.keys(spec.dimensions);
  const dimIndex = new Map(dimNames.map((name, index) => [name, index]));

  const header = [];
  const int = (value) => header.push({ kind: 'int', value });
  const name = (text) => header.push({ kind: 'name', bytes: encoder.encode(text) });
  const attributes = (attrs) => {
    const entries = Object.entries(attrs || {});
    if (entries.length === 0) {
      int(0);
      int(0);
      return;
    }
    int(NC_ATTRIBUTE);
    int(entries.length);
    for (const [key, value] of entries) {
      name(key);
      if (typeof value === 'string') {
        const bytes = encoder.encode(value);
        int(TYPES.char.code);
        int(bytes.length);
        header.push({ kind: 'bytes', bytes });
      } else {
        int(TYPES.double.code);
        int(1);
        header.push({ kind: 'double', value });
      }
    }
  };

  header.push({ kind: 'magic' });
  int(0); // numrecs: no record dimension
  int(NC_DIMENSION);
  int(dimNames.length);
  for (const dim of dimNames) {
    name(dim);
    int(spec.dimensions[dim]);
  }
  attributes(spec.attributes);

  int(NC_VARIABLE);
  int(spec.variables.length);
  const begins = [];
  const sizes = [];
  for (const variable of spec.variables) {
    const type = TYPES[variable.type];
    if (!type) throw new Error(`unknown NetCDF type ${variable.type}`);
    const count = variable.dimensions.reduce((n, dim) => {
      if (!dimIndex.has(dim)) throw new Error(`unknown dimension ${dim}`);
      return n * spec.dimensions[dim];
    }, 1);
    if (variable.data.length !== count) {
      throw new Error(`${variable.name} has ${variable.data.length} values, expected ${count}`);
    }
    name(variable.name);
    int(variable.dimensions.length);
    for (const dim of variable.dimensions) int(dimIndex.get(dim));
    attributes(variable.attributes);
    int(type.code);
    const size = pad4(count * type.size);
    sizes.push(size);
    int(size);
    const begin = { kind: 'int', value: 0 };
    header.push(begin);
    begins.push(begin);
  }

  const headerSize = header.reduce((n, item) => {
    switch (item.kind) {
      case 'magic':
      case 'int':
        return n + 4;
      case 'double':
        return n + 8;
      case 'name':
        return n + 4 + pad4(item.bytes.length);
      case 'bytes':
        return n + pad4(item.bytes.length);
      default:
        throw new Error(item.kind);
    }
  }, 0);

  let offset = headerSize;
  begins.forEach((begin, index) => {
    begin.value = offset;
    offset += sizes[index];
  });

  const out = new Uint8Array(offset);
  const view = new DataView(out.buffer);
  let at = 0;
  for (const item of header) {
    switch (item.kind) {
      case 'magic':
        out.set([0x43, 0x44, 0x46, 0x01], at);
        at += 4;
        break;
      case 'int':
        view.setInt32(at, item.value);
        at += 4;
        break;
      case 'double':
        view.setFloat64(at, item.value);
        at += 8;
        break;
      case 'name':
        view.setInt32(at, item.bytes.length);
        out.set(item.bytes, at + 4);
        at += 4 + pad4(item.bytes.length);
        break;
      case 'bytes':
        out.set(item.bytes, at);
        at += pad4(item.bytes.length);
        break;
    }
  }

  spec.variables.forEach((variable, index) => {
    const type = TYPES[variable.type];
    let position = begins[index].value;
    for (let i = 0; i < variable.data.length; i++) {
      type.put(view, position, variable.data[i]);
      position += type.size;
    }
  });
  return out;
}

/**
 * Weather inputs on a rectilinear grid, one file for the whole period, in the
 * layout risico-2023 reads by default: `latitude`/`longitude` on (y, x) and
 * `time` in seconds since 1970-01-01.
 *
 * `times` are unix seconds; each variable is laid out (time, y, x).
 *
 * @param {{ lats: number[], lons: number[], times: number[],
 *   variables: Record<string, Float32Array | number[]> }} input
 */
export function writeWeatherInput({ lats, lons, times, variables }) {
  const ny = lats.length;
  const nx = lons.length;
  const lat2d = new Float32Array(ny * nx);
  const lon2d = new Float32Array(ny * nx);
  for (let y = 0; y < ny; y++) {
    for (let x = 0; x < nx; x++) {
      lat2d[y * nx + x] = lats[y];
      lon2d[y * nx + x] = lons[x];
    }
  }
  return writeNetcdf3({
    dimensions: { time: times.length, y: ny, x: nx },
    attributes: { Conventions: 'CF-1.8', source: 'risico-2023 browser demo' },
    variables: [
      {
        name: 'latitude', type: 'float', dimensions: ['y', 'x'],
        attributes: { units: 'degrees_north' }, data: lat2d,
      },
      {
        name: 'longitude', type: 'float', dimensions: ['y', 'x'],
        attributes: { units: 'degrees_east' }, data: lon2d,
      },
      {
        name: 'time', type: 'double', dimensions: ['time'],
        attributes: { units: 'seconds since 1970-01-01 00:00:00' }, data: times,
      },
      ...Object.entries(variables).map(([name, data]) => ({
        name, type: 'float', dimensions: ['time', 'y', 'x'], data,
      })),
    ],
  });
}
