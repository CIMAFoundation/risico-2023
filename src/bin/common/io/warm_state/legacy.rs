use std::io::BufRead;

use chrono::{DateTime, NaiveDateTime, Utc};
use risico::constants::NODATAVAL;
use risico::modules::fwi::models::FWIWarmState;
use risico::modules::risico::models::RISICOWarmState;

use crate::common::helpers::RISICOError;

pub fn read_risico(
    reader: impl BufRead,
    source: &str,
) -> Result<Vec<RISICOWarmState>, RISICOError> {
    let mut states = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line
            .map_err(|error| format!("cannot read {source} at line {}: {error}", line_index + 1))?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('%') {
            continue;
        }
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() != 7 && columns.len() != 9 {
            return Err(format!(
                "invalid RISICO warm state in {source} at line {}: expected 7 or 9 columns, found {}",
                line_index + 1,
                columns.len()
            )
            .into());
        }
        let value = |index: usize, name: &str| -> Result<f32, RISICOError> {
            columns[index].parse::<f32>().map_err(|error| {
                format!(
                    "invalid {name} in {source} at line {}: {error}",
                    line_index + 1
                )
                .into()
            })
        };
        states.push(RISICOWarmState {
            dffm: value(0, "dffm")?,
            snow_cover: value(1, "snow_cover")?,
            snow_cover_time: value(2, "snow_cover_time")?,
            MSI: value(3, "MSI")?,
            MSI_TTL: value(4, "MSI_TTL")?,
            NDVI: value(5, "NDVI")?,
            NDVI_TIME: value(6, "NDVI_TIME")?,
            NDWI: if columns.len() == 9 {
                value(7, "NDWI")?
            } else {
                NODATAVAL
            },
            NDWI_TIME: if columns.len() == 9 {
                value(8, "NDWI_TIME")?
            } else {
                0.0
            },
        });
    }
    if states.is_empty() {
        return Err(format!("RISICO warm state {source} contains no cells").into());
    }
    Ok(states)
}

pub fn read_fwi(
    reader: impl BufRead,
    source: &str,
    snapshot_time: DateTime<Utc>,
) -> Result<Vec<FWIWarmState>, RISICOError> {
    let mut states = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line
            .map_err(|error| format!("cannot read {source} at line {}: {error}", line_index + 1))?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('%') {
            continue;
        }
        let columns: Vec<&str> = line.split_whitespace().collect();
        let parse_values = |raw: &str, name: &str| -> Result<Vec<f32>, RISICOError> {
            raw.split(',')
                .map(|value| {
                    value.parse::<f32>().map_err(|error| {
                        format!(
                            "invalid {name} in {source} at line {}: {error}",
                            line_index + 1
                        )
                        .into()
                    })
                })
                .collect()
        };
        let state = match columns.as_slice() {
            [dates, ffmc, dmc, dc, rain] => FWIWarmState {
                dates: dates
                    .split(',')
                    .map(|value| {
                        NaiveDateTime::parse_from_str(value, "%Y%m%d%H%M")
                            .map(|date| DateTime::from_naive_utc_and_offset(date, Utc))
                            .map_err(|error| {
                                RISICOError::from(format!(
                                    "invalid history date in {source} at line {}: {error}",
                                    line_index + 1
                                ))
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                ffmc: parse_values(ffmc, "FFMC")?,
                dmc: parse_values(dmc, "DMC")?,
                dc: parse_values(dc, "DC")?,
                rain: parse_values(rain, "rain")?,
            },
            // Deployed scalar order is rain, FFMC, DMC, DC.
            [rain, ffmc, dmc, dc] => FWIWarmState {
                dates: vec![snapshot_time],
                ffmc: parse_values(ffmc, "FFMC")?,
                dmc: parse_values(dmc, "DMC")?,
                dc: parse_values(dc, "DC")?,
                rain: parse_values(rain, "rain")?,
            },
            _ => {
                return Err(format!(
                "invalid FWI warm state in {source} at line {}: expected 4 or 5 columns, found {}",
                line_index + 1,
                columns.len()
            )
                .into())
            }
        };
        let length = state.dates.len();
        if length == 0
            || [
                state.ffmc.len(),
                state.dmc.len(),
                state.dc.len(),
                state.rain.len(),
            ]
            .iter()
            .any(|candidate| *candidate != length)
        {
            return Err(format!(
                "FWI warm-state histories have different lengths in {source} at line {}",
                line_index + 1
            )
            .into());
        }
        states.push(state);
    }
    if states.is_empty() {
        return Err(format!("FWI warm state {source} contains no cells").into());
    }
    Ok(states)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use chrono::TimeZone;

    use super::*;

    #[test]
    fn reads_risico_old_and_current_layouts() {
        let states = read_risico(
            Cursor::new("1 2 3 4 5 6 7\n8 9 10 11 12 13 14 15 16\n"),
            "test-state",
        )
        .expect("legacy RISICO state should parse");
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].NDWI, NODATAVAL);
        assert_eq!(states[1].NDWI, 15.0);
        assert_eq!(states[1].NDWI_TIME, 16.0);
    }

    #[test]
    fn rejects_short_risico_rows_without_panicking() {
        let error =
            read_risico(Cursor::new("1 2 3\n"), "test-state").expect_err("short row should fail");
        assert!(error.to_string().contains("expected 7 or 9 columns"));
    }

    #[test]
    fn reads_deployed_fwi_scalar_order() {
        let time = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let states = read_fwi(Cursor::new("1 80 6 15\n"), "test-state", time)
            .expect("deployed FWI state should parse");
        assert_eq!(states[0].dates, vec![time]);
        assert_eq!(states[0].rain, vec![1.0]);
        assert_eq!(states[0].ffmc, vec![80.0]);
    }

    #[test]
    fn reads_fwi_history_layout() {
        let time = Utc.with_ymd_and_hms(2026, 7, 20, 0, 0, 0).unwrap();
        let states = read_fwi(
            Cursor::new("202607190000,202607200000 80,81 6,7 15,16 0,1\n"),
            "test-state",
            time,
        )
        .expect("FWI history state should parse");
        assert_eq!(states[0].dates.len(), 2);
        assert_eq!(states[0].rain, vec![0.0, 1.0]);
    }
}
