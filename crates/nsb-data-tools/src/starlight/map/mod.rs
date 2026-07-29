//! Sparse deterministic HEALPix shard accumulation and reconciliation.

pub mod accumulator;
pub mod product;

#[cfg(test)]
mod tests {
    use super::product::validate_map;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn validation_rejects_incomplete_full_sky_map_declaration() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("starlight_nside1.csv");
        fs::write(
            &path,
            concat!(
                "# schema=nsb-healpix-starlight-candidate-v3\n",
                "# map_type=healpix\n",
                "# coordinate_frame=galactic\n",
                "# ordering=nested\n",
                "# representation=full-sky\n",
                "# omitted_pixel_semantics=not_applicable\n",
                "# nside=1\n",
                "# flux_quantity=integrated_per_pixel\n",
                "# flux_unit=ph_m-2_s-1\n",
                "# derivation=canonical_gaia_source_accumulation\n",
                "# source_count_semantics=exact_source_membership\n",
                "pixel,flux_ph_m2_s,admitted_sources,excluded_sources\n",
                "0,1.0,1,0\n",
            ),
        )
        .unwrap();

        let error = validate_map(&path, 1).unwrap_err().to_string();
        assert!(error.contains("unknown or incompatible map headers"));
        assert!(error.contains("full-sky"));
    }
}
