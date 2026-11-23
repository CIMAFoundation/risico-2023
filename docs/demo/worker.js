// Runs risico-2023 (wasm32-wasip1) off the page's main thread.
import * as shim from 'https://cdn.jsdelivr.net/npm/@bjorn3/browser_wasi_shim@0.4.1/dist/index.js';
import { runRisico } from './lib/runner.js';

let compiled = null;

self.onmessage = async ({ data }) => {
  if (data.type !== 'run') return;
  try {
    compiled ??= WebAssembly.compileStreaming(fetch(data.wasmUrl));
    const module = await compiled;
    self.postMessage({ type: 'started' });
    const started = performance.now();
    const result = await runRisico({
      shim,
      module,
      files: new Map(data.files),
      args: data.args,
      onLine: (line) => self.postMessage({ type: 'log', line }),
    });
    const outputs = [];
    for (const [path, bytes] of result.files) {
      if (path.startsWith('output/') && path.endsWith('.nc')) outputs.push([path, bytes.slice()]);
    }
    self.postMessage(
      {
        type: 'done',
        exitCode: result.exitCode,
        memoryBytes: result.memoryBytes,
        seconds: (performance.now() - started) / 1000,
        outputs,
      },
      outputs.map(([, bytes]) => bytes.buffer),
    );
  } catch (error) {
    self.postMessage({ type: 'error', message: String(error && error.message ? error.message : error) });
  }
};
