//! `Spectrum` mirroring the Python helper class.
//!
//! A `Spectrum` is a pair of equally-sized vectors `(lambda, flux)` plus
//! optional uncertainty. Wavelengths are stored in nanometres unless noted.

#[derive(Debug, Clone)]
pub struct Spectrum {
    pub lambda_nm: Vec<f64>,
    pub flux: Vec<f64>,
    pub tag: Option<String>,
}

impl Spectrum {
    pub fn new(lambda_nm: Vec<f64>, flux: Vec<f64>) -> Self {
        assert_eq!(lambda_nm.len(), flux.len(), "lambda and flux must be same length");
        Self { lambda_nm, flux, tag: None }
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    pub fn len(&self) -> usize { self.lambda_nm.len() }
    pub fn is_empty(&self) -> bool { self.lambda_nm.is_empty() }

    /// Linear interpolation. Out-of-range queries clamp to endpoints
    /// (matches `np.interp`'s default behaviour, which the Python code
    /// implicitly relies on via `interp1d`).
    pub fn interp(&self, lambda_nm: f64) -> f64 {
        let xs = &self.lambda_nm;
        let ys = &self.flux;
        if lambda_nm <= xs[0] { return ys[0]; }
        if lambda_nm >= *xs.last().unwrap() { return *ys.last().unwrap(); }
        let i = xs.partition_point(|&x| x <= lambda_nm);
        let (x0, x1) = (xs[i - 1], xs[i]);
        let (y0, y1) = (ys[i - 1], ys[i]);
        let t = (lambda_nm - x0) / (x1 - x0);
        y0 + t * (y1 - y0)
    }

    /// Trapezoidal integral over the full range.
    pub fn integrate(&self) -> f64 {
        let xs = &self.lambda_nm;
        let ys = &self.flux;
        let mut s = 0.0;
        for i in 1..xs.len() {
            s += 0.5 * (ys[i] + ys[i - 1]) * (xs[i] - xs[i - 1]);
        }
        s
    }

    /// Trapezoidal integral over `[lo, hi]` (in nm).
    pub fn integrate_range(&self, lo_nm: f64, hi_nm: f64) -> f64 {
        let xs = &self.lambda_nm;
        let mut s = 0.0;
        for i in 1..xs.len() {
            let (a, b) = (xs[i - 1], xs[i]);
            if b < lo_nm || a > hi_nm { continue; }
            let lo = a.max(lo_nm);
            let hi = b.min(hi_nm);
            let ya = self.interp(lo);
            let yb = self.interp(hi);
            s += 0.5 * (ya + yb) * (hi - lo);
        }
        s
    }
}
