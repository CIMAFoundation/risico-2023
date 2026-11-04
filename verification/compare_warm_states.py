#!/usr/bin/env python3
"""Compare a legacy TXT warm state with its NetCDF replacement."""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path

import numpy as np


RISICO_FIELDS = (
    ("dffm", 0),
    ("snow_cover", 1),
    ("snow_cover_time", 2),
    ("msi", 3),
    ("msi_ttl", 4),
    ("ndvi", 5),
    ("ndvi_time", 6),
    ("ndwi", 7),
    ("ndwi_time", 8),
)
FWI_FIELDS = (("rain", 0), ("ffmc", 1), ("dmc", 2), ("dc", 3))


def ncdump(*args: str | Path) -> str:
    command = ["ncdump", *(str(arg) for arg in args)]
    return subprocess.check_output(command, text=True)


def read_variable(path: Path, name: str) -> np.ndarray:
    output = ncdump("-p", "9", "-v", name, path)
    data = output.split("data:", 1)[1]
    body = data.split(f"{name} =", 1)[1].rsplit(";", 1)[0]
    # ncdump renders fill values as `_`; normalize them to NaN for comparison.
    return np.fromstring(body.replace("_", "nan"), sep=",", dtype=np.float32)


def state_timestamp(path: Path) -> int:
    header = ncdump("-h", path)
    match = re.search(r":state_time\s*=\s*(-?\d+)LL\s*;", header)
    if not match:
        raise ValueError(f"missing state_time in {path}")
    return int(match.group(1))


def timestamp_from_name(path: Path) -> int:
    match = re.search(r"(\d{12})(?:\D.*)?$", path.name)
    if not match:
        raise ValueError(f"TXT filename does not end in YYYYmmddHHMM: {path}")
    from datetime import datetime, timezone

    parsed = datetime.strptime(match.group(1), "%Y%m%d%H%M").replace(
        tzinfo=timezone.utc
    )
    return int(parsed.timestamp())


def read_legacy(path: Path, model: str) -> np.ndarray:
    if model == "risico":
        return np.loadtxt(path, dtype=np.float32, ndmin=2)

    rows: list[tuple[float, float, float, float]] = []
    with path.open() as source:
        for line_number, line in enumerate(source, 1):
            columns = line.split()
            if len(columns) == 4:
                rain, ffmc, dmc, dc = (float(value) for value in columns)
            elif len(columns) == 5:
                # Legacy-mode NetCDF intentionally stores the effective scalar
                # state: first moisture code and most recent rain observation.
                _, ffmc_values, dmc_values, dc_values, rain_values = columns
                ffmc = float(ffmc_values.split(",")[0])
                dmc = float(dmc_values.split(",")[0])
                dc = float(dc_values.split(",")[0])
                rain = float(rain_values.split(",")[-1])
            else:
                raise ValueError(
                    f"{path}:{line_number} has {len(columns)} columns; expected 4 or 5"
                )
            rows.append((rain, ffmc, dmc, dc))
    return np.asarray(rows, dtype=np.float32)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("model", choices=("risico", "fwi"))
    parser.add_argument("txt", type=Path)
    parser.add_argument("netcdf", type=Path)
    parser.add_argument("--atol", type=float, default=1.0e-5)
    args = parser.parse_args()

    fields = RISICO_FIELDS if args.model == "risico" else FWI_FIELDS
    expected_columns = 9 if args.model == "risico" else 4
    legacy = read_legacy(args.txt, args.model)
    if legacy.shape[1] != expected_columns:
        raise ValueError(
            f"{args.txt} has {legacy.shape[1]} columns; expected {expected_columns}"
        )

    txt_time = timestamp_from_name(args.txt)
    nc_time = state_timestamp(args.netcdf)
    print(f"model={args.model} cells={legacy.shape[0]} atol={args.atol:g}")
    print(f"state_time: txt={txt_time} netcdf={nc_time} equal={txt_time == nc_time}")

    failed = txt_time != nc_time
    active = read_variable(args.netcdf, "active").astype(bool)
    if int(active.sum()) != legacy.shape[0]:
        raise ValueError(
            f"NetCDF has {int(active.sum())} active pixels; TXT has {legacy.shape[0]} rows"
        )
    history_count = (
        read_variable(args.netcdf, "history_count").astype(np.uint32)
        if args.model == "fwi"
        else None
    )
    print("field differing max_abs_diff first_cell")
    for name, column in fields:
        grid_values = read_variable(args.netcdf, name)
        if args.model == "risico":
            actual = grid_values[active]
        else:
            grid_size = active.size
            histories = grid_values.reshape((-1, grid_size))
            active_indexes = np.flatnonzero(active)
            counts = history_count[active_indexes]
            if np.any(counts == 0) or np.any(counts > histories.shape[0]):
                raise ValueError(f"invalid FWI history_count values in {args.netcdf}")
            history_indexes = counts - 1 if name == "rain" else np.zeros_like(counts)
            actual = histories[history_indexes, active_indexes]
        expected = legacy[:, column]
        if actual.shape != expected.shape:
            print(f"{name} LENGTH {actual.size} != {expected.size}")
            failed = True
            continue

        both_nan = np.isnan(actual) & np.isnan(expected)
        finite_pair = np.isfinite(actual) & np.isfinite(expected)
        difference = np.zeros(actual.shape, dtype=np.float64)
        difference[finite_pair] = np.abs(
            actual[finite_pair].astype(np.float64) - expected[finite_pair].astype(np.float64)
        )
        mismatch = ~(both_nan | (finite_pair & (difference <= args.atol)))
        indexes = np.flatnonzero(mismatch)
        max_difference = float(difference[finite_pair].max(initial=0.0))
        first = str(int(indexes[0])) if indexes.size else "-"
        print(f"{name} {indexes.size} {max_difference:.9g} {first}")
        failed |= bool(indexes.size)

    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
