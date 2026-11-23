// Hourly weather from Open-Meteo (https://open-meteo.com), shaped as the
// risico-2023 inputs: T [°C], H [%], W [m/s], D [deg, from], P [mm/h],
// SNOW [cm]. The request goes straight from the visitor's browser.

const API = 'https://api.open-meteo.com/v1/forecast';
const HOURLY = ['temperature_2m', 'relative_humidity_2m', 'wind_speed_10m', 'wind_direction_10m', 'precipitation', 'snow_depth'];
const BATCH = 100;

const toInputs = (hourly, index) => {
  const value = (name, scale = 1) => {
    const v = hourly[name][index];
    return v === null || v === undefined ? NaN : v * scale;
  };
  return [
    value('temperature_2m'),
    value('relative_humidity_2m'),
    value('wind_speed_10m'),
    value('wind_direction_10m'),
    value('precipitation'),
    value('snow_depth', 100),
  ];
};

async function request(lats, lons, { pastDays, forecastDays, signal, onWait }) {
  const params = new URLSearchParams({
    latitude: lats.map((v) => v.toFixed(3)).join(','),
    longitude: lons.map((v) => v.toFixed(3)).join(','),
    hourly: HOURLY.join(','),
    wind_speed_unit: 'ms',
    timeformat: 'unixtime',
    timezone: 'GMT',
    past_days: String(pastDays),
    forecast_days: String(forecastDays),
  });
  let response = await fetch(`${API}?${params}`, { signal });
  for (let retry = 0; response.status === 429 && retry < 2; retry++) {
    // Free-tier limits are per minute: wait and try again.
    onWait?.(65);
    await new Promise((resolve) => setTimeout(resolve, 65000));
    response = await fetch(`${API}?${params}`, { signal });
  }
  if (!response.ok) {
    let reason = response.statusText;
    try {
      reason = (await response.json()).reason || reason;
    } catch {}
    throw new Error(`Open-Meteo: ${response.status} ${reason}`);
  }
  const body = await response.json();
  return Array.isArray(body) ? body : [body];
}

/**
 * Weather at a set of points, all on the same hourly timeline.
 *
 * @param {Array<[number, number]>} points [lat, lon]
 * @returns {Promise<{ times: number[], values: Float32Array[] }>} `values[v]`
 *   is laid out (time, point) for v in T, H, W, D, P, SNOW.
 */
export async function fetchPoints(points, { pastDays = 7, forecastDays = 3, signal, onProgress = () => {}, onWait } = {}) {
  let times = null;
  let values = null;
  for (let start = 0; start < points.length; start += BATCH) {
    const batch = points.slice(start, start + BATCH);
    const results = await request(batch.map((p) => p[0]), batch.map((p) => p[1]), { pastDays, forecastDays, signal, onWait });
    results.forEach((result, offset) => {
      if (!times) {
        times = result.hourly.time;
        values = Array.from({ length: 6 }, () => new Float32Array(times.length * points.length));
      }
      const point = start + offset;
      for (let t = 0; t < times.length; t++) {
        const row = toInputs(result.hourly, t);
        for (let v = 0; v < 6; v++) values[v][t * points.length + point] = row[v];
      }
    });
    onProgress(Math.min(points.length, start + BATCH), points.length);
  }
  fillGaps(times.length, points.length, values);
  return { times, values };
}

/** Carry the last valid value forward (then backward) over missing hours. */
function fillGaps(ntimes, npoints, values) {
  for (const series of values) {
    for (let p = 0; p < npoints; p++) {
      let last = NaN;
      for (let t = 0; t < ntimes; t++) {
        const i = t * npoints + p;
        if (Number.isNaN(series[i])) series[i] = last;
        else last = series[i];
      }
      last = NaN;
      for (let t = ntimes - 1; t >= 0; t--) {
        const i = t * npoints + p;
        if (Number.isNaN(series[i])) series[i] = Number.isNaN(last) ? 0 : last;
        else last = series[i];
      }
    }
  }
}

/** Hourly weather at one location. */
export async function fetchCell(lat, lon, options) {
  const { times, values } = await fetchPoints([[lat, lon]], options);
  const [T, H, W, D, P, SNOW] = values;
  return { times, variables: { T, H, W, D, P, SNOW } };
}
