// Run the risico-2023 WASI binary on an in-memory filesystem.
//
// The filesystem comes from @bjorn3/browser_wasi_shim, passed in as `shim` so
// the same code runs in a Web Worker (shim loaded from a CDN) and in Node
// tests (shim from npm).

let patched = false;

/**
 * The shim grows a file by reallocating it on every write past its end, which
 * is quadratic for the multi-megabyte NetCDF outputs. Keep a doubling
 * capacity behind `file.data` instead; `file.data` stays an exact-length view,
 * so every other shim method keeps working unchanged.
 */
function patchShim(shim) {
  if (patched) return;
  patched = true;
  const grow = (file, end) => {
    if (end <= file.data.byteLength) return;
    const buffer = file._buffer;
    if (buffer && file.data.buffer === buffer.buffer && file.data.byteOffset === 0 && end <= buffer.byteLength) {
      file.data = buffer.subarray(0, end);
      return;
    }
    const capacity = Math.max(end, Math.min(file.data.byteLength * 2, file.data.byteLength + (256 << 20)), 64 << 10);
    const next = new Uint8Array(capacity);
    next.set(file.data);
    file._buffer = next;
    file.data = next.subarray(0, end);
  };
  const ERRNO_BADF = 8;
  shim.OpenFile.prototype.fd_write = function (data) {
    if (this.file.readonly) return { ret: ERRNO_BADF, nwritten: 0 };
    const position = Number(this.file_pos);
    grow(this.file, position + data.byteLength);
    this.file.data.set(data, position);
    this.file_pos += BigInt(data.byteLength);
    return { ret: 0, nwritten: data.byteLength };
  };
  shim.OpenFile.prototype.fd_pwrite = function (data, offset) {
    if (this.file.readonly) return { ret: ERRNO_BADF, nwritten: 0 };
    const position = Number(offset);
    grow(this.file, position + data.byteLength);
    this.file.data.set(data, position);
    return { ret: 0, nwritten: data.byteLength };
  };
}

function buildTree(shim, files) {
  const root = new Map();
  for (const [path, bytes] of files) {
    const parts = path.split('/').filter((part) => part && part !== '.');
    let dir = root;
    for (const part of parts.slice(0, -1)) {
      let entry = dir.get(part);
      if (!entry) {
        entry = new shim.Directory(new Map());
        dir.set(part, entry);
      }
      dir = entry.contents;
    }
    dir.set(parts[parts.length - 1], new shim.File(bytes));
  }
  return root;
}

function collect(shim, contents, prefix, out) {
  for (const [name, entry] of contents) {
    const path = prefix ? `${prefix}/${name}` : name;
    if (entry instanceof shim.Directory) collect(shim, entry.contents, path, out);
    else out.set(path, entry.data);
  }
  return out;
}

/**
 * @param {{
 *   shim: any,
 *   module: WebAssembly.Module,
 *   files: Map<string, Uint8Array>,   // relative paths under the working directory
 *   args: string[],                   // arguments after the program name
 *   env?: Record<string, string>,
 *   onLine?: (line: string) => void,  // stdout and stderr, line by line
 * }} options
 * @returns {Promise<{ exitCode: number, files: Map<string, Uint8Array>, memoryBytes: number }>}
 */
export async function runRisico({ shim, module, files, args, env = {}, onLine = () => {} }) {
  patchShim(shim);
  const contents = buildTree(shim, files);
  const environment = Object.entries({ RUST_LOG: 'info', TMPDIR: '/tmp', ...env }).map(([k, v]) => `${k}=${v}`);
  if (!contents.has('tmp')) contents.set('tmp', new shim.Directory(new Map()));
  const fds = [
    new shim.OpenFile(new shim.File([])),
    shim.ConsoleStdout.lineBuffered(onLine),
    shim.ConsoleStdout.lineBuffered(onLine),
    new shim.PreopenDirectory('/', contents),
  ];
  const wasi = new shim.WASI(['risico-2023', ...args], environment, fds, { debug: false });
  const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: wasi.wasiImport });
  let exitCode;
  try {
    exitCode = wasi.start(instance);
  } catch (error) {
    onLine(`trap: ${error && error.message ? error.message : error}`);
    exitCode = 134;
  }
  return {
    exitCode,
    files: collect(shim, contents, '', new Map()),
    memoryBytes: instance.exports.memory.buffer.byteLength,
  };
}
