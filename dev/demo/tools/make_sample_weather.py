"""Build the demo's sample weather: hourly Open-Meteo historical forecasts on
the 0.125 deg grid over the Liguria cut-out, written as a risico-2023 NetCDF
input (latitude/longitude on (y, x), time in seconds since 1970, T H W D P SNOW).

    uv run --no-project --with netCDF4 --with numpy --with requests \
        docs/demo/tools/make_sample_weather.py 2025-07-01 2025-07-31 \
        docs/demo/data/sample/weather-2025-07.nc
"""

import sys
import time

import netCDF4
import numpy as np
import requests

API = "https://historical-forecast-api.open-meteo.com/v1/forecast"
HOURLY = ["temperature_2m", "relative_humidity_2m", "wind_speed_10m",
          "wind_direction_10m", "precipitation", "snow_depth"]
# name, Open-Meteo variable, scale, units, long name
OUTPUTS = [
    ("T", "temperature_2m", 1, "degC", "air temperature at 2 m"),
    ("H", "relative_humidity_2m", 1, "%", "relative humidity at 2 m"),
    ("W", "wind_speed_10m", 1, "m s-1", "wind speed at 10 m"),
    ("D", "wind_direction_10m", 1, "degree", "wind direction at 10 m (from)"),
    ("P", "precipitation", 1, "mm", "precipitation in the past hour"),
    ("SNOW", "snow_depth", 100, "cm", "snow depth"),
]
LATS = np.round(np.arange(43.625, 44.875 + 1e-6, 0.125), 3)
LONS = np.round(np.arange(7.375, 10.25 + 1e-6, 0.125), 3)
BATCH = 40


def fetch(points, start, end):
    params = {
        "latitude": ",".join(f"{p[0]:.3f}" for p in points),
        "longitude": ",".join(f"{p[1]:.3f}" for p in points),
        "hourly": ",".join(HOURLY),
        "wind_speed_unit": "ms",
        "timeformat": "unixtime",
        "timezone": "GMT",
        "start_date": start,
        "end_date": end,
    }
    for attempt in range(5):
        response = requests.get(API, params=params, timeout=120)
        if response.status_code == 429:
            time.sleep(65)
            continue
        response.raise_for_status()
        body = response.json()
        return body if isinstance(body, list) else [body]
    raise RuntimeError("Open-Meteo kept refusing the request")


def main(start, end, path):
    points = [(lat, lon) for lat in LATS for lon in LONS]
    results = []
    for i in range(0, len(points), BATCH):
        results += fetch(points[i:i + BATCH], start, end)
        print(f"{len(results)} / {len(points)} points", file=sys.stderr)
        time.sleep(20)
    times = np.array(results[0]["hourly"]["time"], dtype="f8")
    ny, nx, nt = len(LATS), len(LONS), len(times)

    with netCDF4.Dataset(path, "w", format="NETCDF4") as ds:
        ds.Conventions = "CF-1.8"
        ds.title = f"risico-2023 demo sample weather, Liguria, {start} to {end}"
        ds.source = "Open-Meteo historical forecast API (https://open-meteo.com), CC BY 4.0"
        ds.createDimension("time", None)
        ds.createDimension("y", ny)
        ds.createDimension("x", nx)
        lat2d, lon2d = np.meshgrid(LATS, LONS, indexing="ij")
        v = ds.createVariable("latitude", "f4", ("y", "x"))
        v.units = "degrees_north"
        v[:] = lat2d
        v = ds.createVariable("longitude", "f4", ("y", "x"))
        v.units = "degrees_east"
        v[:] = lon2d
        v = ds.createVariable("time", "f8", ("time",))
        v.units = "seconds since 1970-01-01 00:00:00"
        v[:] = times
        for name, source, scale, units, long_name in OUTPUTS:
            data = np.full((nt, ny, nx), np.nan, dtype="f4")
            for p, result in enumerate(results):
                series = np.array([np.nan if x is None else x for x in result["hourly"][source]], dtype="f4")
                data[:, p // nx, p % nx] = series * scale
            # fill gaps along time, as the browser does for live data
            for y in range(ny):
                for x in range(nx):
                    s = data[:, y, x]
                    ok = ~np.isnan(s)
                    if ok.any() and not ok.all():
                        idx = np.arange(nt)
                        data[:, y, x] = np.interp(idx, idx[ok], s[ok])
            v = ds.createVariable(name, "f4", ("time", "y", "x"), zlib=True, complevel=6,
                                  chunksizes=(24, ny, nx))
            v.units = units
            v.long_name = long_name
            v[:] = data
    print(f"wrote {path}: {nt} hours on {ny} x {nx} nodes", file=sys.stderr)


if __name__ == "__main__":
    main(*sys.argv[1:4])
