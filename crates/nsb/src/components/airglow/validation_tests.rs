use super::AirglowContinuum;

const RAW: &str = include_str!("../../../data/airglow_cont.dat");

fn assert_rejected(source: &str, needle: &str) {
    let error = source
        .parse::<AirglowContinuum>()
        .expect_err("malformed Airglow calibration must be rejected");
    assert!(
        error.to_string().contains(needle),
        "expected {needle:?} in {error}"
    );
}

#[test]
fn parser_rejects_invalid_header_arity_and_numbers() {
    for (source, needle) in [
        (
            RAW.replacen("\n6 3\n", "\n6\n", 1),
            "nseason/ntime row has 1 values, expected 2",
        ),
        (
            RAW.replacen("\n6 3\n", "\nsix 3\n", 1),
            "invalid nseason/ntime value",
        ),
        (
            RAW.replacen("\n6 3\n", "\n5 3\n", 1),
            "unsupported correction dimensions",
        ),
        (
            RAW.replacen("\n46\n", "\n46 47\n", 1),
            "ndat row has 2 values, expected 1",
        ),
        (
            RAW.replacen("\n46\n", "\nnot-a-count\n", 1),
            "invalid ndat value",
        ),
        (
            RAW.replacen("\n90\n", "\n90 91\n", 1),
            "height row has 2 values, expected 1",
        ),
        (
            RAW.replacen("\n90\n", "\nheight\n", 1),
            "bad numeric value in height",
        ),
        (
            RAW.replacen("2.068E-01 6.139E-03", "2.068E-01", 1),
            "expected 2 solar-activity values, got 1",
        ),
    ] {
        assert_rejected(&source, needle);
    }
}

#[test]
fn parser_rejects_malformed_matrix_and_spectral_rows() {
    let mean_row = "0.998 0.809 0.916 1.24 1.071 1.036 0.918";
    let bad_mean = RAW.replacen(mean_row, "0.998 0.809 0.916 1.24 1.071 1.036", 1);
    assert_rejected(
        &bad_mean,
        "mean corrections row 0 has 6 columns, expected 7",
    );

    let first_sample = "0.3 0.92 0.65";
    let bad_sample = RAW.replacen(first_sample, "0.3 0.92", 1);
    assert_rejected(&bad_sample, "spectral data row 0 has 2 columns, expected 3");

    let trailing = format!("{RAW}\n0.1 1.0 0.1\n");
    assert_rejected(
        &trailing,
        "unexpected trailing data after declared spectral samples",
    );
}

#[test]
fn validated_boundary_rejects_remaining_invalid_numeric_domains() {
    let sigma_row = "0.382 0.305 0.255 0.458 0.379 0.448 0.232";
    let negative_sigma = RAW.replacen(sigma_row, "-0.382 0.305 0.255 0.458 0.379 0.448 0.232", 1);
    assert_rejected(&negative_sigma, "must be non-negative");

    let first_sample = "0.3 0.92 0.65";
    let zero_wavelength = RAW.replacen(first_sample, "0.0 0.92 0.65", 1);
    assert_rejected(
        &zero_wavelength,
        "wavelength sample 0 must be finite and greater than zero",
    );

    let non_finite_mean = RAW.replacen(first_sample, "0.3 NaN 0.65", 1);
    assert_rejected(&non_finite_mean, "relative mean sample 0 must be finite");

    let non_finite_solar = RAW.replacen("2.068E-01 6.139E-03", "NaN 6.139E-03", 1);
    assert_rejected(
        &non_finite_solar,
        "solar-activity intercept and slope must be finite",
    );
}

#[test]
fn validated_boundary_rejects_spectrum_with_fewer_than_two_samples() {
    let one_sample = r#"
6 3
1
90
79.829
2.068E-01 6.139E-03
0.998 0.809 0.916 1.24 1.071 1.036 0.918
0.982 0.847 1.045 1.115 1.095 0.955 0.832
0.905 0.773 0.81 1.256 0.896 0.84 0.876
1.108 0.829 0.895 1.35 1.232 1.321 1.035
0.382 0.305 0.255 0.458 0.379 0.448 0.232
0.32 0.202 0.248 0.302 0.343 0.474 0.158
0.368 0.189 0.199 0.598 0.301 0.353 0.181
0.424 0.469 0.258 0.411 0.466 0.375 0.287
0.3 0.92 0.65
"#;
    assert_rejected(one_sample, "at least two wavelength samples");
}
