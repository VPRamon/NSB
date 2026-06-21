use super::output::StarlightOutputs;
use super::photometry::bilinear_outputs;
use super::provenance::StarlightProvenance;
use crate::error::{NsbError, Result};
use qtty::angular::Degrees;
use qtty::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s};
use siderust::coordinates::cartesian::Direction as CartesianDirection;
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};
use std::collections::BTreeMap;
use std::path::Path;

const EPS: f64 = 1.0e-10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarlightPixel {
    pub galactic_lon: Degrees,
    pub galactic_lat: Degrees,
    pub solid_angle_sr: f64,
    pub integrated: BandPhotonRadiance,
    pub b_flux_s10: S10s,
    pub v_flux_s10: S10s,
}

impl StarlightPixel {
    pub fn new(
        galactic_lon: Degrees,
        galactic_lat: Degrees,
        solid_angle_sr: f64,
        integrated: BandPhotonRadiance,
        b_flux_s10: S10s,
        v_flux_s10: S10s,
    ) -> Self {
        Self {
            galactic_lon,
            galactic_lat,
            solid_angle_sr,
            integrated,
            b_flux_s10,
            v_flux_s10,
        }
    }

    fn output(self) -> StarlightOutputs {
        StarlightOutputs::new(self.integrated, self.b_flux_s10, self.v_flux_s10)
    }

    fn normalized(self) -> Self {
        Self {
            galactic_lon: Degrees::new(normalize_lon_deg(self.galactic_lon.value())),
            ..self
        }
    }

    fn validate(self) -> Result<()> {
        if !self.galactic_lon.is_finite() || !self.galactic_lat.is_finite() {
            return Err(invalid_map("pixel coordinates must be finite"));
        }
        if !(-90.0..=90.0).contains(&self.galactic_lat.value()) {
            return Err(invalid_map(format!(
                "galactic latitude {} deg is outside [-90, 90]",
                self.galactic_lat.value()
            )));
        }
        if !self.solid_angle_sr.is_finite() || self.solid_angle_sr <= 0.0 {
            return Err(invalid_map(
                "pixel solid_angle_sr must be finite and positive",
            ));
        }
        if !self.output().is_finite_non_negative() {
            return Err(invalid_map(
                "pixel radiance and S10 values must be finite and non-negative",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StarlightMap {
    provenance: StarlightProvenance,
    kind: StarlightMapKind,
}

#[derive(Debug, Clone, PartialEq)]
enum StarlightMapKind {
    Rectangular {
        lon_values_deg: Vec<f64>,
        lat_values_deg: Vec<f64>,
        pixels: Vec<StarlightPixel>,
    },
    Healpix {
        grid: HealpixGrid,
        pixels: Vec<StarlightPixel>,
    },
}

impl StarlightMap {
    pub fn from_pixels(
        pixels: Vec<StarlightPixel>,
        provenance: StarlightProvenance,
    ) -> Result<Self> {
        if pixels.is_empty() {
            return Err(invalid_map("starlight map must contain at least one pixel"));
        }

        let mut normalized = Vec::with_capacity(pixels.len());
        let mut lon_values = Vec::with_capacity(pixels.len());
        let mut lat_values = Vec::with_capacity(pixels.len());
        for pixel in pixels {
            pixel.validate()?;
            let pixel = pixel.normalized();
            lon_values.push(pixel.galactic_lon.value());
            lat_values.push(pixel.galactic_lat.value());
            normalized.push(pixel);
        }

        sort_dedup(&mut lon_values);
        sort_dedup(&mut lat_values);

        let expected_len = lon_values.len() * lat_values.len();
        if expected_len != normalized.len() {
            return Err(invalid_map(format!(
                "starlight map must be rectangular: {} longitudes x {} latitudes != {} pixels",
                lon_values.len(),
                lat_values.len(),
                normalized.len()
            )));
        }

        let mut grid = vec![None; expected_len];
        for pixel in normalized {
            let lon_idx = axis_index(&lon_values, pixel.galactic_lon.value())?;
            let lat_idx = axis_index(&lat_values, pixel.galactic_lat.value())?;
            let idx = grid_index(lon_values.len(), lon_idx, lat_idx);
            if grid[idx].replace(pixel).is_some() {
                return Err(invalid_map(format!(
                    "duplicate starlight pixel at l={} deg, b={} deg",
                    pixel.galactic_lon.value(),
                    pixel.galactic_lat.value()
                )));
            }
        }

        let mut pixels = Vec::with_capacity(expected_len);
        for value in grid {
            pixels.push(value.ok_or_else(|| invalid_map("rectangular map has a missing pixel"))?);
        }

        Ok(Self {
            provenance,
            kind: StarlightMapKind::Rectangular {
                lon_values_deg: lon_values,
                lat_values_deg: lat_values,
                pixels,
            },
        })
    }

    pub fn from_csv_str(raw: &str, provenance: StarlightProvenance) -> Result<Self> {
        let metadata = parse_header_metadata(raw);
        let provenance = StarlightProvenance::from_header_metadata(&metadata, provenance);
        let data_header = first_data_header(raw)?;

        if data_header.starts_with("healpix_index,") {
            Self::from_healpix_csv_str(raw, metadata, provenance)
        } else if data_header.starts_with("galactic_lon_deg,") {
            Self::from_rectangular_csv_str(raw, provenance)
        } else {
            Err(NsbError::DataParse {
                file: "starlight map csv",
                message: format!("unsupported starlight map header {data_header:?}"),
            })
        }
    }

    pub fn from_csv_path(path: impl AsRef<Path>, provenance: StarlightProvenance) -> Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_csv_str(&raw, provenance)
    }

    pub fn lookup(&self, galactic_lon: Degrees, galactic_lat: Degrees) -> StarlightOutputs {
        match &self.kind {
            StarlightMapKind::Rectangular {
                lon_values_deg,
                lat_values_deg,
                pixels,
            } => {
                let (lon0, lon1, tx) = lon_bracket(lon_values_deg, galactic_lon.value());
                let (lat0, lat1, ty) = bracket_clamped(lat_values_deg, galactic_lat.value());

                bilinear_outputs(
                    pixels[grid_index(lon_values_deg.len(), lon0, lat0)].output(),
                    pixels[grid_index(lon_values_deg.len(), lon1, lat0)].output(),
                    pixels[grid_index(lon_values_deg.len(), lon0, lat1)].output(),
                    pixels[grid_index(lon_values_deg.len(), lon1, lat1)].output(),
                    tx,
                    ty,
                )
            }
            StarlightMapKind::Healpix { grid, pixels } => {
                let direction = galactic_cartesian_direction(galactic_lon.value(), galactic_lat.value());
                let index = grid
                    .direction_to_pixel(direction)
                    .expect("validated HEALPix lookup direction is finite");
                pixels[usize::try_from(index.get()).expect("pixel index fits usize")].output()
            }
        }
    }

    pub fn provenance(&self) -> &StarlightProvenance {
        &self.provenance
    }

    pub fn pixels(&self) -> &[StarlightPixel] {
        match &self.kind {
            StarlightMapKind::Rectangular { pixels, .. } | StarlightMapKind::Healpix { pixels, .. } => {
                pixels
            }
        }
    }

    fn from_rectangular_csv_str(raw: &str, provenance: StarlightProvenance) -> Result<Self> {
        let mut pixels = Vec::new();
        let mut saw_header = false;

        for (line_idx, line) in raw.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if !saw_header && line.starts_with("galactic_lon_deg,") {
                saw_header = true;
                continue;
            }
            saw_header = true;

            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            if fields.len() != 6 {
                return Err(NsbError::DataParse {
                    file: "starlight map csv",
                    message: format!(
                        "line {} has {} fields, expected 6",
                        line_idx + 1,
                        fields.len()
                    ),
                });
            }

            let parse = |idx: usize, name: &str| -> Result<f64> {
                fields[idx]
                    .parse::<f64>()
                    .map_err(|err| NsbError::DataParse {
                        file: "starlight map csv",
                        message: format!("line {} invalid {name}: {err}", line_idx + 1),
                    })
            };

            pixels.push(StarlightPixel::new(
                Degrees::new(parse(0, "galactic_lon_deg")?),
                Degrees::new(parse(1, "galactic_lat_deg")?),
                parse(2, "solid_angle_sr")?,
                BandPhotonRadiance::new(parse(3, "integrated_ph_cm2_ns_sr")?),
                S10s::new(parse(4, "b_s10")?),
                S10s::new(parse(5, "v_s10")?),
            ));
        }

        Self::from_pixels(pixels, provenance)
    }

    fn from_healpix_csv_str(
        raw: &str,
        metadata: BTreeMap<String, String>,
        provenance: StarlightProvenance,
    ) -> Result<Self> {
        let nside = required_metadata(&metadata, "nside")?
            .parse::<u32>()
            .map_err(|err| NsbError::DataParse {
                file: "starlight map csv",
                message: format!("invalid HEALPix nside: {err}"),
            })?;
        let ordering = match required_metadata(&metadata, "ordering")?.to_ascii_lowercase().as_str() {
            "ring" => HealpixOrdering::Ring,
            "nested" => HealpixOrdering::Nested,
            other => {
                return Err(NsbError::DataParse {
                    file: "starlight map csv",
                    message: format!("unsupported HEALPix ordering {other:?}"),
                })
            }
        };
        let frame = required_metadata(&metadata, "coordinate_frame")?.to_ascii_lowercase();
        if frame != "galactic" {
            return Err(NsbError::DataParse {
                file: "starlight map csv",
                message: format!("expected coordinate_frame=galactic, got {frame:?}"),
            });
        }

        let grid = HealpixGrid::new(Nside::new(nside).map_err(|err| invalid_map(err.to_string()))?, ordering)
            .map_err(|err| invalid_map(err.to_string()))?;
        let npix = usize::try_from(grid.npix()).expect("HEALPix npix fits usize");
        let mut pixels = vec![None; npix];
        let mut saw_header = false;

        for (line_idx, line) in raw.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !saw_header && line.starts_with("healpix_index,") {
                saw_header = true;
                continue;
            }
            saw_header = true;

            let fields: Vec<&str> = line.split(',').map(str::trim).collect();
            if fields.len() != 4 {
                return Err(NsbError::DataParse {
                    file: "starlight map csv",
                    message: format!(
                        "line {} has {} fields, expected 4",
                        line_idx + 1,
                        fields.len()
                    ),
                });
            }
            let index = parse_u64(fields[0], line_idx + 1, "healpix_index")?;
            grid.validate_index(HealpixIndex::new(index))
                .map_err(|err| invalid_map(err.to_string()))?;
            let slot = usize::try_from(index).expect("pixel index fits usize");
            let (lon, lat) = healpix_pixel_lon_lat_deg(grid, HealpixIndex::new(index))?;
            let pixel = StarlightPixel::new(
                Degrees::new(lon),
                Degrees::new(lat),
                grid.pixel_area_sr(),
                BandPhotonRadiance::new(parse_f64(fields[1], line_idx + 1, "integrated_ph_cm2_ns_sr")?),
                S10s::new(parse_f64(fields[2], line_idx + 1, "b_s10")?),
                S10s::new(parse_f64(fields[3], line_idx + 1, "v_s10")?),
            );
            pixel.validate()?;
            if pixels[slot].replace(pixel).is_some() {
                return Err(invalid_map(format!("duplicate HEALPix pixel index {index}")));
            }
        }

        let mut validated = Vec::with_capacity(npix);
        for (index, pixel) in pixels.into_iter().enumerate() {
            validated.push(pixel.ok_or_else(|| invalid_map(format!("missing HEALPix pixel index {index}")))?);
        }

        Ok(Self {
            provenance,
            kind: StarlightMapKind::Healpix {
                grid,
                pixels: validated,
            },
        })
    }
}

fn first_data_header(raw: &str) -> Result<&str> {
    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .ok_or_else(|| invalid_map("starlight map csv has no data header"))
}

fn parse_header_metadata(raw: &str) -> BTreeMap<String, String> {
    raw.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .filter_map(|line| line.trim_start_matches('#').trim().split_once('='))
        .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn required_metadata<'a>(metadata: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    metadata
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| invalid_map(format!("missing required HEALPix metadata key {key:?}")))
}

fn parse_u64(raw: &str, line: usize, name: &str) -> Result<u64> {
    raw.parse::<u64>().map_err(|err| NsbError::DataParse {
        file: "starlight map csv",
        message: format!("line {line} invalid {name}: {err}"),
    })
}

fn parse_f64(raw: &str, line: usize, name: &str) -> Result<f64> {
    raw.parse::<f64>().map_err(|err| NsbError::DataParse {
        file: "starlight map csv",
        message: format!("line {line} invalid {name}: {err}"),
    })
}

fn healpix_pixel_lon_lat_deg(grid: HealpixGrid, index: HealpixIndex) -> Result<(f64, f64)> {
    let direction: CartesianDirection<Galactic> = grid
        .pixel_center(index)
        .map_err(|err| invalid_map(err.to_string()))?;
    let [x, y, z] = direction.as_array();
    let lon = normalize_lon_deg(y.atan2(x).to_degrees());
    let lat = z.clamp(-1.0, 1.0).asin().to_degrees();
    Ok((lon, lat))
}

fn galactic_cartesian_direction(lon_deg: f64, lat_deg: f64) -> CartesianDirection<Galactic> {
    let lon = lon_deg.to_radians();
    let lat = lat_deg.to_radians();
    let cos_lat = lat.cos();
    CartesianDirection::<Galactic>::from_array([cos_lat * lon.cos(), cos_lat * lon.sin(), lat.sin()])
}

fn bracket_clamped(values: &[f64], x: f64) -> (usize, usize, f64) {
    if values.len() == 1 {
        return (0, 0, 0.0);
    }
    if x <= values[0] {
        return (0, 0, 0.0);
    }
    let last = values.len() - 1;
    if x >= values[last] {
        return (last, last, 0.0);
    }
    for i in 0..last {
        let lo = values[i];
        let hi = values[i + 1];
        if x + EPS >= lo && x <= hi + EPS {
            let tx = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
            return (i, i + 1, tx);
        }
    }
    (last, last, 0.0)
}

fn lon_bracket(values: &[f64], lon_deg: f64) -> (usize, usize, f64) {
    if values.len() == 1 {
        return (0, 0, 0.0);
    }

    let x = normalize_lon_deg(lon_deg);
    let last = values.len() - 1;
    for i in 0..values.len() {
        let j = if i == last { 0 } else { i + 1 };
        let lo = values[i];
        let mut hi = values[j];
        let mut x_adj = x;
        if i == last {
            hi += 360.0;
            if x_adj < lo {
                x_adj += 360.0;
            }
        }
        if x_adj + EPS >= lo && x_adj <= hi + EPS {
            let tx = if (hi - lo).abs() <= EPS {
                0.0
            } else {
                ((x_adj - lo) / (hi - lo)).clamp(0.0, 1.0)
            };
            return (i, j, tx);
        }
    }

    let nearest = values
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| circular_distance(**a, x).total_cmp(&circular_distance(**b, x)))
        .map(|(idx, _)| idx)
        .expect("validated map has at least one longitude");
    (nearest, nearest, 0.0)
}

fn sort_dedup(values: &mut Vec<f64>) {
    values.sort_by(|a, b| a.total_cmp(b));
    values.dedup_by(|a, b| (*a - *b).abs() <= EPS);
}

fn axis_index(values: &[f64], value: f64) -> Result<usize> {
    values
        .iter()
        .position(|&candidate| (candidate - value).abs() <= EPS)
        .ok_or_else(|| invalid_map("internal map axis lookup failed"))
}

fn grid_index(n_lon: usize, lon_idx: usize, lat_idx: usize) -> usize {
    lat_idx * n_lon + lon_idx
}

fn normalize_lon_deg(value: f64) -> f64 {
    let mut out = value % 360.0;
    if out < 0.0 {
        out += 360.0;
    }
    if (out - 360.0).abs() <= EPS {
        0.0
    } else {
        out
    }
}

fn circular_distance(a: f64, b: f64) -> f64 {
    let diff = (a - b).abs() % 360.0;
    diff.min(360.0 - diff)
}

fn invalid_map(message: impl Into<String>) -> NsbError {
    NsbError::InvalidMap {
        message: message.into(),
    }
}
