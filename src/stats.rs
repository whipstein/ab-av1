use std::fmt::Display;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Stats {
    pub mean: f32,
    pub harmonic_mean: f32,
    pub median: f32,
    pub min: f32,
    pub max: f32,
    pub range: f32,
    pub sum: f32,
    pub size: usize,
    pub std_dev: f32,
    pub variance: f32,
    pub midrange: f32,
    pub q1: f32,
    pub q3: f32,
    pub upper_fence: f32,
    pub lower_fence: f32,
    pub eff_min: f32,
    pub eff_max: f32,
}

impl Stats {
    fn new() -> Self {
        Stats {
            mean: 0.0,
            harmonic_mean: 0.0,
            median: 0.0,
            min: 0.0,
            max: 0.0,
            range: 0.0,
            sum: 0.0,
            size: 0,
            std_dev: 0.0,
            variance: 0.0,
            midrange: 0.0,
            q1: 0.0,
            q3: 0.0,
            upper_fence: 0.0,
            lower_fence: 0.0,
            eff_min: 0.0,
            eff_max: 0.0,
        }
    }

    pub fn calc_stats(input: &Vec<f32>) -> Self {
        let mut vals = input.clone();

        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let size = vals.len();
        let min = vals[0].clone();
        let max = vals[size - 1];
        let range = max - min;
        let midrange = (min - max) / 2.0;
        let q1 = vals[(size + 1) / 4];
        let q3 = vals[(3 * size + 3) / 4];
        let upper_fence = q3 + 1.5 * (q3 - q1);
        let lower_fence = q1 - 1.5 * (q3 - q1);
        let mut mean: f64 = 0.0;
        let mut harmonic_mean: f64 = 0.0;
        let mut median = 0.0;
        let mut sum: f64 = 0.0;
        let mut std_dev: f64 = 0.0;
        let mut variance: f64 = 0.0;
        let mut eff_min = 0.0;
        let mut eff_max = 0.0;

        let midpoint = size / 2;

        if size % 2 == 0 {
            median = (vals[size / 2 - 1] + vals[size / 2]) / 2.0;
        } else {
            median = vals[size / 2];
        }

        if min < lower_fence {
            eff_min = lower_fence.clone();
        } else {
            eff_min = min.clone();
        }

        if max > upper_fence {
            eff_max = upper_fence.clone();
        } else {
            eff_max = max.clone();
        }

        for val in vals.iter() {
            sum += *val as f64;
            harmonic_mean += 1.0 / *val as f64;
        }
        mean = sum / size as f64;
        harmonic_mean = size as f64 / harmonic_mean;

        for val in vals.iter() {
            variance = (*val as f64 - mean).powi(2);
        }
        variance /= size as f64;
        std_dev = variance.clone().sqrt();

        Stats {
            mean: mean as f32,
            harmonic_mean: harmonic_mean as f32,
            median,
            min,
            max,
            range,
            sum: sum as f32,
            size,
            std_dev: std_dev as f32,
            variance: variance as f32,
            midrange,
            q1,
            q3,
            upper_fence,
            lower_fence,
            eff_min,
            eff_max,
        }
    }
}

impl Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Summary Statistics\n\tMean:\t\t\t{}\n\tMedian:\t\t\t{}\n\tHarmonic Mean:\t\t{}\n\tStandard Deviation:\t{}\n\tEff Min:\t\t{}\n\tEff Max:\t\t{}\n\tLF:\t\t\t{}\n\tUF:\t\t\t{}\n\tSize:\t\t\t{}",
            self.mean,
            self.median,
            self.harmonic_mean,
            self.std_dev,
            self.eff_min,
            self.eff_max,
            self.lower_fence,
            self.upper_fence,
            self.size,
        )
    }
}
