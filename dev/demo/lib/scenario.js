// Turn a demo scenario (cells, static layers, hourly weather) into the files,
// configuration and arguments of one risico-2023 run.

import { writeWeatherInput } from './netcdf3.js';
import { writeGeoTiff } from './geotiff-write.js';

/** Output variables per model: [internal name, file name, precision], the
 * precision being the power of ten values are rounded to (-1 keeps 0.1). */
export const MODEL_OUTPUTS = {
  RISICO: [
    ['dffm', 'dffm', -1], ['W', 'W', -2], ['V', 'V', 0], ['I', 'I', 0],
    ['contrT', 'contrT', -3],
  ],
  INPUTS: [
    ['temperature', 'T', -1], ['humidity', 'H', -1], ['windSpeed', 'WS', -1],
    ['windDir', 'WD', 0], ['rain', 'P', -1], ['snowCover', 'SNOW', -1],
  ],
  FWI: [['ffmc', 'ffmc', -1], ['dmc', 'dmc', -1], ['dc', 'dc', -1], ['isi', 'isi', -2], ['bui', 'bui', -1], ['fwi', 'fwi', -2]],
  KBDI: [['kbdi', 'kbdi', -1]],
  Mark5: [['df', 'df', -2], ['ffdi', 'ffdi', -2]],
  Angstrom: [['angstrom', 'angstrom', -2]],
  Fosberg: [['ffwi', 'ffwi', -2]],
  Nesterov: [['nesterov', 'nesterov', 0]],
  Sharples: [['fmi', 'fmi', -2], ['f', 'sharples_f', -2]],
  Orieux: [['pet_t', 'pet_t', -2], ['orieux_wr', 'orieux_wr', -1], ['orieux_fd', 'orieux_fd', 0]],
  Hdw: [['hdw', 'hdw', -1]],
};

/** Models driven by a plain `lon lat value` cell list, with the third column. */
const CELL_MODELS = {
  KBDI: 'mean_rain', Mark5: 'mean_rain', Angstrom: 'none', Fosberg: 'none',
  Nesterov: 'none', Sharples: 'none', Orieux: 'heat_index', Hdw: 'none',
};
const WARM_STATE_MODELS = new Set(['KBDI', 'Mark5', 'Nesterov', 'Orieux']);
const HOURLY_MODELS = new Set(['Angstrom', 'Fosberg', 'Sharples', 'Hdw']);

const STATIC_LAYERS = ['slope', 'aspect', 'vegetation_id', 'ppf_summer', 'ppf_winter'];

const yaml = (value, indent = '') => {
  if (Array.isArray(value)) {
    return value.map((item) => {
      const body = yaml(item, `${indent}  `).replace(/^\s+/, '');
      return `${indent}- ${body}`;
    }).join('\n');
  }
  if (value && typeof value === 'object') {
    return Object.entries(value).map(([key, item]) => {
      if (item && typeof item === 'object') {
        if (Array.isArray(item) && item.length === 0) return `${indent}${key}: []`;
        if (!Array.isArray(item) && Object.keys(item).length === 0) return `${indent}${key}: {}`;
        return `${indent}${key}:\n${yaml(item, `${indent}  `)}`;
      }
      return `${indent}${key}: ${JSON.stringify(item)}`;
    }).join('\n');
  }
  return `${indent}${JSON.stringify(value)}`;
};

const pad = (n) => String(n).padStart(2, '0');
const runDate = (date) => `${date.getUTCFullYear()}${pad(date.getUTCMonth() + 1)}${pad(date.getUTCDate())}${pad(date.getUTCHours())}${pad(date.getUTCMinutes())}`;

/**
 * @param {{
 *   raster: { width, height, originLon, originLat, pixelWidth, pixelHeight },
 *     // the static-data grid; `mask`, `layers.*` are laid out on it
 *   mask: Float32Array,
 *   layers: Record<string, Float32Array>,   // slope (deg), aspect (deg), vegetation_id, ppf_*
 *   vegetationCatalog: string,              // p_vegetazione.csv
 *   outputGrid: { nrows, ncols, min_lat, max_lat, min_lon, max_lon },
 *   weather: { lats, lons, times, variables },  // see writeWeatherInput
 *   models: string[],                        // keys of MODEL_OUTPUTS, without INPUTS
 *   cellValues?: { mean_rain?: number, heat_index?: number },
 *   outputEveryHours?: number,
 *   outputs?: string[],                      // internal names to write (default: all)
 * }} scenario
 */
export function buildRun(scenario) {
  const files = new Map();
  const { raster } = scenario;
  files.set('static/domain_mask.tif', writeGeoTiff({ ...raster, values: scenario.mask }));
  for (const name of STATIC_LAYERS) {
    if (scenario.layers[name]) {
      files.set(`static/${name}.tif`, writeGeoTiff({ ...raster, values: scenario.layers[name] }));
    }
  }
  files.set('static/vegetation.csv', new TextEncoder().encode(scenario.vegetationCatalog));
  files.set('input/weather.nc', writeWeatherInput(scenario.weather));

  // Cell list for the index models: centres of the active mask pixels.
  const cellLines = [];
  for (let row = 0; row < raster.height; row++) {
    for (let col = 0; col < raster.width; col++) {
      if (!scenario.mask[row * raster.width + col]) continue;
      const lon = raster.originLon + (col + 0.5) * raster.pixelWidth;
      const lat = raster.originLat - (row + 0.5) * raster.pixelHeight;
      cellLines.push([lon.toFixed(6), lat.toFixed(6)]);
    }
  }
  const cellValues = { none: 0, mean_rain: 800, heat_index: 70, ...scenario.cellValues };

  const outputEvery = scenario.outputEveryHours ?? 1;
  const output = (model, extra = []) => ({
    internal_name: model,
    name: model,
    path: `output/${model}`,
    grid: scenario.outputGrid,
    format: 'NETCDF',
    compression_level: 1,
    variables: [...MODEL_OUTPUTS[model], ...extra]
      .filter(([internal]) => !scenario.outputs || scenario.outputs.includes(internal))
      .map(([internal, name, precision]) => ({
        internal_name: internal,
        name,
        cluster_mode: 'Mean',
        precision,
      })),
  });

  const models = [];
  const staticData = {
    type: 'geotiff',
    domain_mask: 'static/domain_mask.tif',
    vegetation_catalog: 'static/vegetation.csv',
    ...Object.fromEntries(STATIC_LAYERS.filter((name) => scenario.layers[name]).map((name) => [name, `static/${name}.tif`])),
  };
  const startDate = new Date(scenario.weather.times[0] * 1000);
  for (const model of scenario.models) {
    if (model === 'RISICO') {
      models.push({
        type: 'RISICO', model_name: 'RISICO', model_version: 'v2025',
        static_data: staticData,
        warm_state: { type: 'netcdf', directory: 'state/RISICO', on_missing: 'defaults' },
        output_time_resolution: outputEvery,
        output_types: [output('RISICO', MODEL_OUTPUTS.INPUTS)],
      });
    } else if (model === 'FWI') {
      models.push({
        type: 'FWI', model_name: 'FWI', model_version: 'legacy',
        static_data: staticData,
        warm_state: { type: 'netcdf', directory: 'state/FWI', on_missing: 'defaults' },
        output_time_resolution: Math.max(outputEvery, 1),
        output_types: [output('FWI')],
      });
    } else if (CELL_MODELS[model]) {
      const third = cellValues[CELL_MODELS[model]];
      const path = `static/cells-${model}.txt`;
      files.set(path, new TextEncoder().encode(cellLines.map(([lon, lat]) => `${lon} ${lat} ${third}`).join('\n') + '\n'));
      const entry = { type: model, model_name: model, cells_file_path: path, output_types: [output(model)] };
      if (WARM_STATE_MODELS.has(model)) entry.warm_state_path = `state/${model}/state`;
      if (HOURLY_MODELS.has(model)) entry.output_time_resolution = outputEvery;
      if (model === 'KBDI' || model === 'Mark5') entry.model_version = 'legacy';
      models.push(entry);
    } else {
      throw new Error(`unknown model ${model}`);
    }
  }

  const config = yaml({
    streaming: { overlap_postprocess: false },
    models,
    palettes: {},
  }) + '\n';
  files.set('config.yml', new TextEncoder().encode(config));
  for (const model of scenario.models) files.set(`state/${model}/.keep`, new Uint8Array(0));
  for (const model of scenario.models) files.set(`output/${model}/.keep`, new Uint8Array(0));

  // The warm-state lookup ends one day before the run date, so starting the
  // run a day after the first input keeps every hour of weather in the run.
  const date = new Date(startDate.getTime());
  return { files, config, args: [runDate(date), 'config.yml', 'input'] };
}
