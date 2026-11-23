// RISICO in the browser: the risico-2023 binary built for wasm32-wasip1, run
// in a Web Worker on an in-memory filesystem. Inputs are NetCDF files and
// GeoTIFF layers written here; outputs are the model's own NetCDF-4 files,
// read back with h5wasm.

import { buildRun } from './lib/scenario.js';
import { fetchCell, fetchPoints } from './lib/openmeteo.js';
import { fromArrayBuffer } from 'https://cdn.jsdelivr.net/npm/geotiff@2.1.3/+esm';
import h5wasm from 'https://cdn.jsdelivr.net/npm/h5wasm@0.7.8/dist/esm/hdf5_hl.js';
import uPlot from 'https://cdn.jsdelivr.net/npm/uplot@1.6.31/dist/uPlot.esm.js';

const L = window.L;
const DATA = new URL('data/', import.meta.url);
const WASM_URL = new URL('risico-2023.wasm', import.meta.url).href;
const NODATA = -9999;
const CELL_MODELS = ['RISICO', 'FWI', 'KBDI', 'Mark5', 'Angstrom', 'Fosberg', 'Nesterov', 'Sharples', 'Orieux', 'Hdw'];
const MAP_MODELS = ['RISICO', 'FWI'];
const MAP_OUTPUTS = ['dffm', 'V', 'I', 'W', 'temperature', 'humidity', 'windSpeed', 'rain', 'ffmc', 'dmc', 'dc', 'isi', 'bui', 'fwi'];

const $ = (id) => document.getElementById(id);
const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const fmtDate = (seconds) => new Date(seconds * 1000).toISOString().slice(0, 16).replace('T', ' ') + ' UTC';

// ---------------------------------------------------------------- state ---

const state = {
  mode: 'scenario',
  statics: null,
  cell: null,
  running: false,
  charts: [],
  mapResult: null,
};

// --------------------------------------------------------- static data ---

const VEGETATION_WORDS = {
  non: 'Non-vegetated', vegetato: '',
  praterie: 'Grassland', foresta: '', latifoglie: 'Broadleaf forest', conifere: 'Conifer forest',
  macchia: 'Mediterranean shrubland', bassa: '· low susceptibility', media: '· medium susceptibility',
  alta: '· high susceptibility', susc: '',
};
const vegetationLabel = (raw) =>
  raw.split(/[_\s]+/).map((word) => (word in VEGETATION_WORDS ? VEGETATION_WORDS[word] : word)).filter(Boolean).join(' ');

async function readLayer(name) {
  const response = await fetch(new URL(`static/${name}.tif`, DATA));
  if (!response.ok) throw new Error(`cannot load ${name}.tif (${response.status})`);
  const tiff = await fromArrayBuffer(await response.arrayBuffer());
  const image = await tiff.getImage();
  const [values] = await image.readRasters();
  const [originLon, originLat] = image.getOrigin();
  const [rx, ry] = image.getResolution();
  return {
    values: Float32Array.from(values),
    grid: { width: image.getWidth(), height: image.getHeight(), originLon, originLat, pixelWidth: rx, pixelHeight: -ry },
  };
}

async function loadStatics() {
  const names = ['domain_mask', 'slope', 'aspect', 'vegetation_id', 'ppf_summer', 'ppf_winter'];
  const [layers, catalog] = await Promise.all([
    Promise.all(names.map(readLayer)),
    fetch(new URL('static/p_vegetazione.csv', DATA)).then((r) => r.text()),
  ]);
  const byName = Object.fromEntries(names.map((name, i) => [name, layers[i].values]));
  const grid = layers[0].grid;
  const active = new Uint8Array(grid.width * grid.height);
  byName.domain_mask.forEach((v, i) => { active[i] = v !== 0 && v !== NODATA ? 1 : 0; });
  const vegetation = catalog.split('\n')
    .filter((line) => line.trim() && !line.startsWith('#'))
    .map((line) => {
      const parts = line.trim().split(/\s+/);
      return { id: Number(parts[0]), name: vegetationLabel(parts[parts.length - 1]) };
    });
  return { grid, active, layers: byName, catalog, vegetation };
}

const palettes = {};
async function palette(file) {
  if (!palettes[file]) {
    const text = await (await fetch(new URL(`palettes/${file}.pal`, DATA))).text();
    palettes[file] = text.split('\n')
      .filter((line) => line.trim() && !line.startsWith('#'))
      .map((line) => line.trim().split(/\s+/).map(Number))
      .filter((row) => row[0] !== NODATA)
      .map(([value, r, g, b, a]) => ({ value, color: [r, g, b, a ?? 255] }));
  }
  return palettes[file];
}

const ramp = (stops) => {
  const colors = [[0, 128, 0], [173, 214, 51], [255, 214, 0], [255, 140, 0], [220, 20, 20], [120, 0, 120]];
  return stops.map((value, i) => ({ value, color: [...colors[i], 255] }));
};
const FWI_PALETTES = {
  fwi: ramp([0, 5.2, 11.2, 21.3, 38, 50]),
  ffmc: ramp([0, 70, 80, 86, 89, 92]),
  dmc: ramp([0, 10, 20, 30, 40, 60]),
  dc: ramp([0, 80, 190, 300, 425, 600]),
  isi: ramp([0, 2, 5, 10, 15, 20]),
  bui: ramp([0, 20, 40, 60, 80, 100]),
};
const MAP_PALETTES = {
  dffm: 'RISICO_UMB', V: 'RISICO_V', I: 'RISICO_I', W: 'RISICO_W',
  T: 'INPUT_TEMPERATURE', H: 'INPUT_HUMIDITY', WS: 'INPUT_WINDSPEED', P: 'INPUT_RAIN',
};
const paletteFor = async (name) => FWI_PALETTES[name] ?? palette(MAP_PALETTES[name]);
const colorAt = (stops, value) => {
  if (value === NODATA || Number.isNaN(value)) return null;
  let color = stops[0].color;
  for (const stop of stops) {
    if (value >= stop.value) color = stop.color;
    else break;
  }
  return color;
};

// ------------------------------------------------------------------ map ---

const map = L.map('map', { zoomSnap: 0.25, attributionControl: true }).setView([41.9, 12.6], 5.5);
L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
  maxZoom: 13,
  attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
}).addTo(map);
let overlay = null;
let cellMarker = null;

const mercator = (lat) => Math.log(Math.tan(Math.PI / 4 + (lat * Math.PI) / 360));
const inverseMercator = (y) => (360 / Math.PI) * Math.atan(Math.exp(y)) - 90;

/**
 * Draw a lat/lon raster as a Web Mercator image: each output row is sampled
 * from the source row at its latitude, so the overlay stays registered over
 * the whole 12° of latitude.
 */
function rasterImage({ width, height, south, north, northUp, color }) {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  const image = context.createImageData(width, height);
  const top = mercator(north);
  const bottom = mercator(south);
  for (let y = 0; y < height; y++) {
    const lat = inverseMercator(top - ((y + 0.5) / height) * (top - bottom));
    const fraction = (north - lat) / (north - south);
    let row = Math.min(height - 1, Math.max(0, Math.floor(fraction * height)));
    if (!northUp) row = height - 1 - row;
    for (let x = 0; x < width; x++) {
      const rgba = color(row * width + x);
      if (!rgba) continue;
      const o = (y * width + x) * 4;
      image.data[o] = rgba[0];
      image.data[o + 1] = rgba[1];
      image.data[o + 2] = rgba[2];
      image.data[o + 3] = rgba[3];
    }
  }
  context.putImageData(image, 0, 0);
  return canvas.toDataURL();
}

function showOverlay(url, bounds, opacity = 0.75) {
  if (overlay) overlay.setUrl(url).setBounds(L.latLngBounds(bounds)).setOpacity(opacity);
  else overlay = L.imageOverlay(url, bounds, { opacity, interactive: false }).addTo(map);
}

function staticBounds(grid) {
  return {
    south: grid.originLat - grid.height * grid.pixelHeight,
    north: grid.originLat,
    west: grid.originLon,
    east: grid.originLon + grid.width * grid.pixelWidth,
  };
}

const VEGETATION_COLORS = [
  null,
  [233, 222, 140], [222, 198, 92], [207, 170, 48],
  [150, 200, 120], [94, 168, 84], [46, 125, 50],
  [196, 178, 120], [164, 140, 70], [130, 104, 40],
  [110, 160, 140], [56, 118, 100], [20, 80, 66],
];
function showVegetation() {
  const { grid, layers, active } = state.statics;
  const b = staticBounds(grid);
  const url = rasterImage({
    width: grid.width, height: grid.height, south: b.south, north: b.north, northUp: true,
    color: (i) => {
      if (!active[i]) return null;
      const c = VEGETATION_COLORS[layers.vegetation_id[i]];
      return c ? [...c, 255] : [200, 200, 200, 140];
    },
  });
  showOverlay(url, [[b.south, b.west], [b.north, b.east]], 0.6);
  $('map-tools').classList.remove('visible');
}

function selectCell(lat, lon) {
  const { grid, layers, active, vegetation } = state.statics;
  const col = Math.floor((lon - grid.originLon) / grid.pixelWidth);
  const row = Math.floor((grid.originLat - lat) / grid.pixelHeight);
  if (col < 0 || row < 0 || col >= grid.width || row >= grid.height) {
    setStatus('That point is outside the RISICO domain.', true);
    return;
  }
  const index = row * grid.width + col;
  const cellLat = grid.originLat - (row + 0.5) * grid.pixelHeight;
  const cellLon = grid.originLon + (col + 0.5) * grid.pixelWidth;
  const value = (name) => layers[name][index];
  const vegetationId = active[index] ? value('vegetation_id') : 0;
  state.cell = {
    lat: cellLat, lon: cellLon, row, col, active: !!active[index],
    slope: Math.max(0, value('slope')), aspect: Math.max(0, value('aspect')), vegetationId,
    ppfSummer: value('ppf_summer'), ppfWinter: value('ppf_winter'),
  };
  const vegetationName = vegetation.find((v) => v.id === vegetationId)?.name ?? `class ${vegetationId}`;
  $('cell-info').innerHTML = `
    <h2>Cell ${cellLat.toFixed(3)}° N, ${cellLon.toFixed(3)}° E</h2>
    <dl>
      <dt>Domain</dt><dd>${state.cell.active ? 'active' : 'outside the active domain'}</dd>
      <dt>Vegetation</dt><dd>${vegetationName}</dd>
      <dt>Slope</dt><dd>${state.cell.slope.toFixed(1)}°</dd>
      <dt>Aspect</dt><dd>${state.cell.aspect.toFixed(0)}°</dd>
    </dl>`;
  $('sc-veg').value = String(vegetationId);
  setRange('sc-slope', Math.round(Math.min(60, state.cell.slope)));
  setRange('sc-aspect', Math.round(state.cell.aspect / 10) * 10 % 360);

  const half = [grid.pixelHeight / 2, grid.pixelWidth / 2];
  const bounds = [[cellLat - half[0], cellLon - half[1]], [cellLat + half[0], cellLon + half[1]]];
  if (cellMarker) cellMarker.remove();
  cellMarker = L.layerGroup([
    L.rectangle(bounds, { color: css('--accent'), weight: 2, fill: false }),
    L.circleMarker([cellLat, cellLon], { radius: 6, color: css('--accent'), weight: 2, fillOpacity: 0.2 }),
  ]).addTo(map);
}

// ------------------------------------------------------------- controls ---

function setRange(id, value) {
  $(id).value = value;
  $(id).dispatchEvent(new Event('input'));
}
for (const input of document.querySelectorAll('input[type="range"]')) {
  const out = $(`${input.id}-v`);
  if (!out) continue;
  const update = () => { out.textContent = input.value; };
  input.addEventListener('input', update);
  update();
}
{
  const start = new Date();
  start.setUTCMonth(6, 1);
  if (start > new Date()) start.setUTCFullYear(start.getUTCFullYear() - 1);
  $('sc-start').value = start.toISOString().slice(0, 10);
}

function setMode(mode) {
  if (state.running) return;
  state.mode = mode;
  for (const tab of document.querySelectorAll('.tab')) tab.setAttribute('aria-selected', String(tab.dataset.mode === mode));
  for (const box of document.querySelectorAll('.controls')) box.classList.toggle('visible', box.dataset.for === mode);
  $('run').textContent = mode === 'map' ? 'Run RISICO + FWI over Italy' : 'Run all indices for this cell';
  if (mode === 'map' && state.mapResult) showMapFrame();
  else if (state.statics) showVegetation();
}
for (const tab of document.querySelectorAll('.tab')) tab.addEventListener('click', () => setMode(tab.dataset.mode));

function setStatus(text, error = false) {
  $('status').textContent = text;
  $('status').classList.toggle('error', error);
}
function log(line) {
  const pre = $('log');
  pre.textContent += `${line}\n`;
  if (pre.textContent.length > 200000) pre.textContent = pre.textContent.slice(-150000);
}

// --------------------------------------------------------------- worker ---

let worker = null;
function runModel(run, { totalSteps, onPhase }) {
  worker ??= new Worker(new URL('worker.js', import.meta.url), { type: 'module' });
  return new Promise((resolve, reject) => {
    let steps = 0;
    $('progress').hidden = false;
    $('progress').value = 0;
    worker.onmessage = ({ data }) => {
      if (data.type === 'started') onPhase('Running the model…');
      else if (data.type === 'log') {
        if (!/not found in configuration|^File: |^Variables: /.test(data.line)) log(data.line);
        const step = /^\s*INFO\s+\S+\s+> (\w+): processing \d{4}-\d\d-\d\d \d\d:\d\d/.exec(data.line);
        if (step) {
          steps++;
          $('progress').value = Math.min(1, steps / totalSteps);
          onPhase(`${step[1]}: ${data.line.split('processing ')[1]}`);
        }
      } else if (data.type === 'done') {
        $('progress').value = 1;
        resolve(data);
      } else if (data.type === 'error') reject(new Error(data.message));
    };
    worker.onerror = (event) => reject(new Error(event.message || 'worker failed'));
    const files = [...run.files];
    worker.postMessage({ type: 'run', wasmUrl: WASM_URL, files, args: run.args }, files.map(([, bytes]) => bytes.buffer));
  });
}

// ------------------------------------------------------ reading outputs ---

let h5 = null;
let h5Counter = 0;
async function openNetcdf(bytes) {
  if (!h5) h5 = await h5wasm.ready;
  const name = `out-${h5Counter++}.nc`;
  h5.FS.writeFile(name, bytes);
  return { file: new h5wasm.File(name, 'r'), name };
}
function closeNetcdf({ file, name }) {
  file.close();
  h5.FS.unlink(name);
}
/** h5wasm hands UTF-8 attribute strings back as Latin-1 ("Â°C"). */
const text = (value) => {
  if (typeof value !== 'string' || !/[\u00c0-\u00ff]/.test(value)) return value ?? '';
  try {
    return new TextDecoder().decode(Uint8Array.from(value, (c) => c.charCodeAt(0)));
  } catch {
    return value;
  }
};
const toNumbers = (values) => Array.from(values, (v) => (typeof v === 'bigint' ? Number(v) : v));

/** Time series of every output variable at the centre of the 3 x 3 output grid. */
async function readCellSeries(outputs) {
  const series = {};
  for (const [path, bytes] of outputs) {
    const [, model, file] = path.split('/');
    const variable = file.replace(/\.nc$/, '');
    const handle = await openNetcdf(bytes);
    try {
      const dataset = handle.file.get(variable);
      const [nt, ny, nx] = dataset.shape;
      const values = dataset.value;
      const centre = Math.floor(ny / 2) * nx + Math.floor(nx / 2);
      const data = [];
      for (let t = 0; t < nt; t++) {
        const v = values[t * ny * nx + centre];
        data.push(v === NODATA ? null : v);
      }
      series[`${model}/${variable}`] = {
        times: toNumbers(handle.file.get('time').value),
        values: data,
        units: text(dataset.attrs.units?.value),
        longName: text(dataset.attrs.long_name?.value) || variable,
      };
    } finally {
      closeNetcdf(handle);
    }
  }
  return series;
}

// --------------------------------------------------------------- charts ---

const SERIES_COLORS = ['#2a6fb0', '#dd7600', '#3c9a5f', '#b0413e', '#7a5cb0', '#8a7a2a'];

function destroyCharts() {
  for (const chart of state.charts) chart.destroy();
  state.charts = [];
  $('charts').innerHTML = '';
}

function addChart(container, { title, note, times, series, nowLine }) {
  const box = document.createElement('div');
  box.className = 'chart';
  box.innerHTML = `<h4></h4><p></p>`;
  box.querySelector('h4').textContent = title;
  box.querySelector('p').textContent = note ?? '';
  container.appendChild(box);
  const scales = [...new Set(series.map((s) => s.scale ?? 'y'))];
  const axisColor = css('--muted');
  const grid = { stroke: css('--grid'), width: 1 };
  const opts = {
    width: Math.max(280, box.clientWidth - 16),
    height: 190,
    scales: { x: { time: true }, ...Object.fromEntries(scales.map((s) => [s, { auto: true }])) },
    axes: [
      { stroke: axisColor, grid, ticks: grid },
      ...scales.map((scale, i) => ({
        scale,
        side: i === 0 ? 3 : 1,
        stroke: axisColor,
        grid: i === 0 ? grid : { show: false },
        ticks: grid,
        size: 50,
        label: series.find((s) => (s.scale ?? 'y') === scale).unit,
        labelSize: 18,
      })),
    ],
    series: [
      { value: (u, v) => (v == null ? '—' : fmtDate(v)) },
      ...series.map((s, i) => ({
        label: s.label,
        scale: s.scale ?? 'y',
        stroke: s.color ?? SERIES_COLORS[i % SERIES_COLORS.length],
        width: 1.75,
        fill: s.bars ? `${s.color ?? SERIES_COLORS[i % SERIES_COLORS.length]}55` : undefined,
        paths: s.bars ? uPlot.paths.bars({ size: [0.9, 8] }) : s.step ? uPlot.paths.stepped({ align: 1 }) : undefined,
        points: { show: times.length <= 31 },
        value: (u, v) => (v == null ? '—' : `${+v.toFixed(2)} ${s.unit ?? ''}`),
      })),
    ],
    hooks: nowLine ? {
      draw: [(u) => {
        const x = u.valToPos(nowLine, 'x', true);
        if (x < u.bbox.left || x > u.bbox.left + u.bbox.width) return;
        const ctx = u.ctx;
        ctx.save();
        ctx.strokeStyle = css('--accent');
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(x, u.bbox.top);
        ctx.lineTo(x, u.bbox.top + u.bbox.height);
        ctx.stroke();
        ctx.restore();
      }],
    } : {},
  };
  const chart = new uPlot(opts, [times, ...series.map((s) => s.values)], box);
  chart.box = box;
  state.charts.push(chart);
}

function chartGroup(title) {
  const group = document.createElement('div');
  group.className = 'chart-group';
  group.innerHTML = `<h3></h3><div class="chart-grid"></div>`;
  group.querySelector('h3').textContent = title;
  $('charts').appendChild(group);
  return group.querySelector('.chart-grid');
}

function showCellCharts(series, { nowLine } = {}) {
  destroyCharts();
  const get = (key) => series[key];
  const chart = (container, title, keys, extra = {}) => {
    const present = keys.filter(([key]) => get(key));
    if (!present.length) return;
    const times = get(present[0][0]).times;
    addChart(container, {
      title,
      note: extra.note ?? (present.length === 1 ? get(present[0][0]).longName : ''),
      times,
      nowLine,
      series: present.map(([key, label, options = {}]) => ({
        label,
        values: get(key).values,
        unit: get(key).units,
        ...options,
      })),
    });
  };

  const weather = chartGroup('Weather (model input)');
  chart(weather, 'Temperature and relative humidity', [
    ['RISICO/T', 'Temperature', { scale: 't' }], ['RISICO/H', 'Humidity', { scale: 'h' }],
  ]);
  chart(weather, 'Wind', [['RISICO/WS', 'Speed', { scale: 'w' }], ['RISICO/WD', 'From', { scale: 'd', step: true }]]);
  chart(weather, 'Rain and snow', [['RISICO/P', 'Rain', { bars: true, scale: 'p' }], ['RISICO/SNOW', 'Snow depth', { scale: 's' }]]);

  const risico = chartGroup('RISICO (hourly)');
  chart(risico, 'Dead fine fuel moisture', [['RISICO/dffm', 'dffm']]);
  chart(risico, 'Rate of spread', [['RISICO/V', 'V']]);
  chart(risico, 'Fireline intensity', [['RISICO/I', 'I']]);
  chart(risico, 'Wind and temperature effects', [['RISICO/W', 'Wind effect', { scale: 'w' }], ['RISICO/contrT', 'Temperature effect', { scale: 't' }]]);

  const fwi = chartGroup('Canadian Fire Weather Index (daily)');
  chart(fwi, 'Moisture codes', [['FWI/ffmc', 'FFMC'], ['FWI/dmc', 'DMC'], ['FWI/dc', 'DC']], { note: 'Fine fuel, duff and drought codes' });
  chart(fwi, 'Fire behaviour indices', [['FWI/isi', 'ISI'], ['FWI/bui', 'BUI'], ['FWI/fwi', 'FWI']], { note: 'Initial spread, build-up and fire weather index' });

  const others = chartGroup('Other fire danger indices');
  chart(others, 'Keetch–Byram Drought Index', [['KBDI/kbdi', 'KBDI']]);
  chart(others, 'McArthur Mark 5', [['Mark5/df', 'Drought factor'], ['Mark5/ffdi', 'FFDI']], { note: 'Forest Fire Danger Index and drought factor' });
  chart(others, 'Angström index', [['Angstrom/angstrom', 'Angström']]);
  chart(others, 'Fosberg Fire Weather Index', [['Fosberg/ffwi', 'FFWI']]);
  chart(others, 'Nesterov ignition index', [['Nesterov/nesterov', 'Nesterov']]);
  chart(others, 'Sharples', [['Sharples/fmi', 'Fuel moisture index'], ['Sharples/sharples_f', 'Fire danger index']]);
  chart(others, 'Orieux', [['Orieux/orieux_wr', 'Water reserve', { scale: 'r' }], ['Orieux/orieux_fd', 'Danger class', { scale: 'c', step: true }]]);
  chart(others, 'Hot–Dry–Windy Index', [['Hdw/hdw', 'HDW']]);
}

addEventListener('resize', () => {
  for (const chart of state.charts) chart.setSize({ width: Math.max(280, chart.box.clientWidth - 16), height: 190 });
});

// ------------------------------------------------------------ scenarios ---

function syntheticWeather() {
  const days = +$('sc-days').value;
  const start = Date.parse(`${$('sc-start').value}T00:00:00Z`) / 1000;
  const tMean = +$('sc-t').value;
  const tRange = +$('sc-tr').value;
  const hMean = +$('sc-h').value;
  const hAmp = Math.max(0, Math.min(25, hMean - 5, 100 - hMean));
  const wind = +$('sc-w').value;
  const from = +$('sc-d').value;
  const rainDay = +$('sc-rd').value;
  const rain = +$('sc-ra').value;
  const hours = days * 24;
  const v = { T: [], H: [], W: [], D: [], P: [], SNOW: [] };
  const times = [];
  for (let h = 0; h < hours; h++) {
    times.push(start + h * 3600);
    const daily = Math.sin((2 * Math.PI * ((h % 24) - 7)) / 24); // peaks at 13 UTC
    v.T.push(tMean + (tRange / 2) * daily);
    v.H.push(hMean - hAmp * daily);
    v.W.push(wind * (0.7 + 0.6 * Math.max(0, daily)));
    v.D.push(from);
    const day = Math.floor(h / 24) + 1;
    v.P.push(day === rainDay && h % 24 >= 12 && h % 24 < 18 ? rain / 6 : 0);
    v.SNOW.push(0);
  }
  return { times, variables: v };
}

function cellRun(weather, overrides = {}) {
  const { grid, catalog } = state.statics;
  const cell = state.cell;
  const raster = {
    width: 1, height: 1,
    originLon: cell.lon - grid.pixelWidth / 2, originLat: cell.lat + grid.pixelHeight / 2,
    pixelWidth: grid.pixelWidth, pixelHeight: grid.pixelHeight,
  };
  const layers = {
    slope: Float32Array.of(overrides.slope ?? cell.slope),
    aspect: Float32Array.of(overrides.aspect ?? cell.aspect),
    vegetation_id: Float32Array.of(overrides.vegetationId ?? cell.vegetationId),
  };
  if (cell.ppfSummer !== NODATA && cell.ppfWinter !== NODATA) {
    layers.ppf_summer = Float32Array.of(cell.ppfSummer);
    layers.ppf_winter = Float32Array.of(cell.ppfWinter);
  }
  return buildRun({
    raster,
    mask: Float32Array.of(1),
    layers,
    vegetationCatalog: catalog,
    outputGrid: {
      nrows: 3, ncols: 3,
      min_lat: cell.lat - grid.pixelHeight, max_lat: cell.lat + grid.pixelHeight,
      min_lon: cell.lon - grid.pixelWidth, max_lon: cell.lon + grid.pixelWidth,
    },
    weather: { lats: [cell.lat], lons: [cell.lon], times: weather.times, variables: weather.variables },
    models: CELL_MODELS,
  });
}

async function runCell(weather, overrides, { nowLine } = {}) {
  const run = cellRun(weather, overrides);
  const result = await runModel(run, {
    totalSteps: weather.times.length * CELL_MODELS.length,
    onPhase: setStatus,
  });
  if (result.exitCode !== 0) log(`risico-2023 exited with ${result.exitCode}`);
  setStatus('Reading the NetCDF outputs…');
  const series = await readCellSeries(result.outputs);
  showCellCharts(series, { nowLine });
  setStatus(`${CELL_MODELS.length} models × ${weather.times.length} hours in ${result.seconds.toFixed(1)} s · wasm memory ${(result.memoryBytes / 2 ** 20).toFixed(0)} MB${result.exitCode ? ' · some models failed, see the log' : ''}`, !!result.exitCode);
}

// ------------------------------------------------------------- map mode ---

// Open-Meteo counts every location as a call (600 a minute on the free
// tier), so the weather is requested on a 0.5° grid and interpolated.
const COARSE = { south: 35.0, north: 47.5, west: 6.0, east: 19.5, step: 0.5 };
const REFINE = 4; // 0.5° / 4 = 0.125°

function weatherNodes() {
  const { grid, active } = state.statics;
  const ny = Math.round((COARSE.north - COARSE.south) / COARSE.step) + 1;
  const nx = Math.round((COARSE.east - COARSE.west) / COARSE.step) + 1;
  const needed = new Uint8Array(ny * nx);
  for (let row = 0; row < grid.height; row += 4) {
    for (let col = 0; col < grid.width; col += 4) {
      if (!active[row * grid.width + col]) continue;
      const lat = grid.originLat - (row + 0.5) * grid.pixelHeight;
      const lon = grid.originLon + (col + 0.5) * grid.pixelWidth;
      const y = (lat - COARSE.south) / COARSE.step;
      const x = (lon - COARSE.west) / COARSE.step;
      for (const yy of [Math.floor(y), Math.ceil(y)]) {
        for (const xx of [Math.floor(x), Math.ceil(x)]) {
          if (yy >= 0 && yy < ny && xx >= 0 && xx < nx) needed[yy * nx + xx] = 1;
        }
      }
    }
  }
  const points = [];
  const nodeOf = [];
  needed.forEach((flag, i) => {
    if (!flag) return;
    nodeOf.push(i);
    points.push([COARSE.south + Math.floor(i / nx) * COARSE.step, COARSE.west + (i % nx) * COARSE.step]);
  });
  return { ny, nx, points, nodeOf };
}

/** Coarse Open-Meteo nodes → full coarse grid (nearest fill) → 0.125° bilinear. */
function griddedWeather({ ny, nx, nodeOf }, { times, values }) {
  const nt = times.length;
  const npoints = nodeOf.length;
  // nearest requested node for every coarse node
  const source = new Int32Array(ny * nx).fill(-1);
  nodeOf.forEach((node, p) => { source[node] = p; });
  for (let i = 0; i < ny * nx; i++) {
    if (source[i] >= 0) continue;
    const y = Math.floor(i / nx), x = i % nx;
    let best = -1, bestD = Infinity;
    nodeOf.forEach((node, p) => {
      const d = (Math.floor(node / nx) - y) ** 2 + ((node % nx) - x) ** 2;
      if (d < bestD) { bestD = d; best = p; }
    });
    source[i] = best;
  }
  const fy = REFINE * (ny - 1) + 1, fx = REFINE * (nx - 1) + 1;
  const lats = Array.from({ length: fy }, (_, i) => COARSE.south + (i * COARSE.step) / REFINE);
  const lons = Array.from({ length: fx }, (_, i) => COARSE.west + (i * COARSE.step) / REFINE);
  const [T, H, W, D, P, SNOW] = values;
  const out = { T: new Float32Array(nt * fy * fx), H: new Float32Array(nt * fy * fx), W: new Float32Array(nt * fy * fx), D: new Float32Array(nt * fy * fx), P: new Float32Array(nt * fy * fx), SNOW: new Float32Array(nt * fy * fx) };
  const coarse = (series, t, y, x) => series[t * npoints + source[y * nx + x]];
  const bilinear = (fn, t, y, x) => {
    const y0 = Math.floor(y / REFINE), x0 = Math.floor(x / REFINE);
    const fyy = (y % REFINE) / REFINE, fxx = (x % REFINE) / REFINE;
    const y1 = Math.min(ny - 1, y0 + 1), x1 = Math.min(nx - 1, x0 + 1);
    return (1 - fyy) * ((1 - fxx) * fn(t, y0, x0) + fxx * fn(t, y0, x1))
      + fyy * ((1 - fxx) * fn(t, y1, x0) + fxx * fn(t, y1, x1));
  };
  const rad = Math.PI / 180;
  const u = (t, y, x) => -coarse(W, t, y, x) * Math.sin(coarse(D, t, y, x) * rad);
  const v = (t, y, x) => -coarse(W, t, y, x) * Math.cos(coarse(D, t, y, x) * rad);
  for (let t = 0; t < nt; t++) {
    for (let y = 0; y < fy; y++) {
      for (let x = 0; x < fx; x++) {
        const i = t * fy * fx + y * fx + x;
        out.T[i] = bilinear((tt, yy, xx) => coarse(T, tt, yy, xx), t, y, x);
        out.H[i] = bilinear((tt, yy, xx) => coarse(H, tt, yy, xx), t, y, x);
        out.P[i] = bilinear((tt, yy, xx) => coarse(P, tt, yy, xx), t, y, x);
        out.SNOW[i] = bilinear((tt, yy, xx) => coarse(SNOW, tt, yy, xx), t, y, x);
        const uu = bilinear(u, t, y, x), vv = bilinear(v, t, y, x);
        out.W[i] = Math.hypot(uu, vv);
        out.D[i] = (Math.atan2(-uu, -vv) / rad + 360) % 360;
      }
    }
  }
  return { lats, lons, times, variables: out };
}

async function runMap() {
  const { grid, layers, active, catalog } = state.statics;
  const nodes = weatherNodes();
  setStatus(`Fetching weather for ${nodes.points.length} grid points…`);
  const fetched = await fetchPoints(nodes.points, {
    pastDays: +$('mp-past').value,
    forecastDays: +$('mp-fc').value,
    onProgress: (done, total) => setStatus(`Fetching weather: ${done} / ${total} grid points`),
    onWait: (seconds) => setStatus(`Open-Meteo rate limit reached: retrying in ${seconds} s…`),
  });
  setStatus('Interpolating the weather to 0.125°…');
  await new Promise((r) => setTimeout(r, 0));
  const weather = griddedWeather(nodes, fetched);
  const mask = new Float32Array(active.length);
  active.forEach((a, i) => { mask[i] = a; });
  const b = staticBounds(grid);
  const outputGrid = {
    nrows: grid.height, ncols: grid.width,
    min_lat: b.south + grid.pixelHeight / 2, max_lat: b.north - grid.pixelHeight / 2,
    min_lon: b.west + grid.pixelWidth / 2, max_lon: b.east - grid.pixelWidth / 2,
  };
  const run = buildRun({
    raster: grid,
    mask,
    layers: { slope: layers.slope, aspect: layers.aspect, vegetation_id: layers.vegetation_id, ppf_summer: layers.ppf_summer, ppf_winter: layers.ppf_winter },
    vegetationCatalog: catalog,
    outputGrid,
    weather,
    models: MAP_MODELS,
    outputEveryHours: 3,
    outputs: MAP_OUTPUTS,
  });
  const result = await runModel(run, { totalSteps: weather.times.length * MAP_MODELS.length, onPhase: setStatus });
  if (result.exitCode !== 0) log(`risico-2023 exited with ${result.exitCode}`);
  setStatus('Opening the NetCDF outputs…');
  await openMapResult(result.outputs, outputGrid, fetched.times);
  setStatus(`2 models × ${weather.times.length} hours × ${active.reduce((n, a) => n + a, 0).toLocaleString('en')} cells in ${result.seconds.toFixed(0)} s · wasm memory ${(result.memoryBytes / 2 ** 20).toFixed(0)} MB`, !!result.exitCode);
}

const MAP_LABELS = {
  'RISICO/V': 'RISICO · rate of spread V', 'RISICO/I': 'RISICO · fireline intensity I',
  'RISICO/dffm': 'RISICO · fine fuel moisture', 'RISICO/W': 'RISICO · wind effect W',
  'RISICO/T': 'Input · temperature', 'RISICO/H': 'Input · relative humidity',
  'RISICO/WS': 'Input · wind speed', 'RISICO/P': 'Input · rain',
  'FWI/fwi': 'FWI · fire weather index', 'FWI/ffmc': 'FWI · FFMC', 'FWI/dmc': 'FWI · DMC',
  'FWI/dc': 'FWI · DC', 'FWI/isi': 'FWI · ISI', 'FWI/bui': 'FWI · BUI',
};

async function openMapResult(outputs, outputGrid, weatherTimes) {
  if (state.mapResult) for (const entry of Object.values(state.mapResult.files)) closeNetcdf(entry.handle);
  const files = {};
  for (const [path, bytes] of outputs) {
    const [, model, file] = path.split('/');
    const variable = file.replace(/\.nc$/, '');
    const handle = await openNetcdf(bytes);
    const dataset = handle.file.get(variable);
    files[`${model}/${variable}`] = {
      handle, dataset, variable,
      times: toNumbers(handle.file.get('time').value),
      units: text(dataset.attrs.units?.value),
    };
  }
  const order = Object.keys(MAP_LABELS).filter((key) => files[key]);
  const select = $('map-variable');
  select.innerHTML = order.map((key) => `<option value="${key}">${MAP_LABELS[key]}</option>`).join('');
  state.mapResult = { files, outputGrid };
  const now = Date.now() / 1000;
  select.onchange = () => {
    const entry = files[select.value];
    const slider = $('map-time');
    slider.max = entry.times.length - 1;
    const upcoming = entry.times.findIndex((t) => t >= now);
    slider.value = upcoming >= 0 ? upcoming : entry.times.length - 1;
    showMapFrame();
  };
  $('map-time').oninput = () => showMapFrame();
  select.value = order.includes('RISICO/V') ? 'RISICO/V' : order[0];
  select.onchange();
}

let frameToken = 0;
async function showMapFrame() {
  const result = state.mapResult;
  if (!result) return;
  const token = ++frameToken;
  const key = $('map-variable').value;
  const entry = result.files[key];
  const t = +$('map-time').value;
  const [, ny, nx] = entry.dataset.shape;
  const frame = entry.dataset.slice([[t, t + 1], [], []]);
  const stops = await paletteFor(entry.variable);
  if (token !== frameToken) return;
  const g = result.outputGrid;
  const halfLat = (g.max_lat - g.min_lat) / (g.nrows - 1) / 2;
  const halfLon = (g.max_lon - g.min_lon) / (g.ncols - 1) / 2;
  const south = g.min_lat - halfLat, north = g.max_lat + halfLat;
  const url = rasterImage({
    width: nx, height: ny, south, north, northUp: false,
    color: (i) => colorAt(stops, frame[i]),
  });
  showOverlay(url, [[south, g.min_lon - halfLon], [north, g.max_lon + halfLon]], 0.8);
  $('map-time-label').textContent = fmtDate(entry.times[t]);
  $('map-tools').classList.add('visible');
  drawLegend(stops, entry.units);
}

function drawLegend(stops, units) {
  const legend = $('map-legend');
  legend.innerHTML = '';
  for (const stop of stops) {
    const swatch = document.createElement('canvas');
    swatch.width = 14;
    swatch.height = 14;
    const ctx = swatch.getContext('2d');
    ctx.fillStyle = `rgba(${stop.color[0]},${stop.color[1]},${stop.color[2]},${stop.color[3] / 255})`;
    ctx.fillRect(0, 0, 14, 14);
    const item = document.createElement('span');
    item.append(swatch, ` ≥ ${stop.value}`);
    legend.append(item);
  }
  const unit = document.createElement('span');
  unit.textContent = units;
  legend.append(unit);
}

function mapPixelSeries(latlng) {
  const result = state.mapResult;
  const g = result.outputGrid;
  const stepLat = (g.max_lat - g.min_lat) / (g.nrows - 1);
  const stepLon = (g.max_lon - g.min_lon) / (g.ncols - 1);
  const row = Math.round((latlng.lat - g.min_lat) / stepLat);
  const col = Math.round((latlng.lng - g.min_lon) / stepLon);
  if (row < 0 || col < 0 || row >= g.nrows || col >= g.ncols) return;
  const key = $('map-variable').value;
  const entry = result.files[key];
  const values = Array.from(entry.dataset.slice([[], [row, row + 1], [col, col + 1]]), (v) => (v === NODATA ? null : v));
  if (values.every((v) => v === null)) return;
  const box = document.createElement('div');
  box.className = 'pixel-popup';
  const opts = {
    width: 300, height: 150,
    scales: { x: { time: true } },
    axes: [{ stroke: '#555' }, { stroke: '#555', size: 44 }],
    legend: { show: false },
    series: [{}, { stroke: '#dd7600', width: 1.75, points: { show: false } }],
  };
  L.popup({ minWidth: 310, maxWidth: 340 }).setLatLng(latlng).setContent(box).openOn(map);
  const title = document.createElement('strong');
  title.textContent = `${MAP_LABELS[key]} (${entry.units}) at ${latlng.lat.toFixed(2)}°, ${latlng.lng.toFixed(2)}°`;
  box.append(title);
  new uPlot(opts, [entry.times, values], box);
}

// ------------------------------------------------------------------ run ---

$('run').addEventListener('click', async () => {
  if (state.running || !state.statics) return;
  if (state.mode !== 'map' && !state.cell) {
    setStatus('Pick a cell on the map first.', true);
    return;
  }
  state.running = true;
  $('run').disabled = true;
  $('log').textContent = '';
  try {
    if (state.mode === 'scenario') {
      await runCell(syntheticWeather(), {
        vegetationId: +$('sc-veg').value, slope: +$('sc-slope').value, aspect: +$('sc-aspect').value,
      });
    } else if (state.mode === 'live') {
      setStatus('Fetching weather from Open-Meteo…');
      const weather = await fetchCell(state.cell.lat, state.cell.lon, { pastDays: +$('lv-past').value, forecastDays: +$('lv-fc').value });
      await runCell(weather, {}, { nowLine: Date.now() / 1000 });
    } else {
      destroyCharts();
      await runMap();
    }
  } catch (error) {
    console.error(error);
    setStatus(String(error.message || error), true);
  } finally {
    state.running = false;
    $('run').disabled = false;
    $('progress').hidden = true;
  }
});

map.on('click', (event) => {
  if (state.mode === 'map') {
    if (state.mapResult) mapPixelSeries(event.latlng);
    return;
  }
  if (state.statics && !state.running) selectCell(event.latlng.lat, event.latlng.lng);
});

// ----------------------------------------------------------------- boot ---

(async () => {
  setStatus('Loading the static layers…');
  try {
    state.statics = await loadStatics();
    $('sc-veg').innerHTML = state.statics.vegetation.map((v) => `<option value="${v.id}">${v.name}</option>`).join('');
    showVegetation();
    selectCell(44.45, 8.86);
    setMode('scenario');
    const head = await fetch(WASM_URL, { method: 'HEAD' });
    setStatus(head.ok ? 'Ready.' : 'risico-2023.wasm is missing next to this page: see "Building the demo".', !head.ok);
  } catch (error) {
    console.error(error);
    setStatus(`Cannot start the demo: ${error.message}`, true);
  }
})();
