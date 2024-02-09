#![allow(non_snake_case)]
use std::fmt::Display;
use std::fs;
use std::path::PathBuf;

use nom::{
    bytes::complete::{tag, take_while},
    character::complete::{alphanumeric1, digit1, line_ending, oct_digit1, space0, space1},
    error::ErrorKind,
    multi::separated_list1,
    sequence::{delimited, tuple},
    Err::Error,
    IResult,
};

use serde::{Deserialize, Serialize};
use serde_json::{Result, Value};

use crate::command::args::Vmaf;

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct VmafSummaryData {
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub harmonic_mean: f32,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct VmafMetrics {
    pub integer_motion2: f32,
    pub integer_motion: f32,
    pub integer_adm2: f32,
    pub integer_adm_scale0: f32,
    pub integer_adm_scale1: f32,
    pub integer_adm_scale2: f32,
    pub integer_adm_scale3: f32,
    pub integer_vif_scale0: f32,
    pub integer_vif_scale1: f32,
    pub integer_vif_scale2: f32,
    pub integer_vif_scale3: f32,
    pub vmaf: f32,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct VmafFrameData {
    pub frameNum: u32,
    pub metrics: VmafMetrics,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct VmafPooledMetrics {
    pub integer_motion2: VmafSummaryData,
    pub integer_motion: VmafSummaryData,
    pub integer_adm2: VmafSummaryData,
    pub integer_adm_scale0: VmafSummaryData,
    pub integer_adm_scale1: VmafSummaryData,
    pub integer_adm_scale2: VmafSummaryData,
    pub integer_adm_scale3: VmafSummaryData,
    pub integer_vif_scale0: VmafSummaryData,
    pub integer_vif_scale1: VmafSummaryData,
    pub integer_vif_scale2: VmafSummaryData,
    pub integer_vif_scale3: VmafSummaryData,
    pub vmaf: VmafSummaryData,
}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct VmafAggregateMetrics {}

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct VmafData {
    pub version: String,
    pub fps: f32,
    pub frames: Vec<VmafFrameData>,
    pub pooled_metrics: VmafPooledMetrics,
    pub aggregate_metrics: VmafAggregateMetrics,
}

impl VmafData {
    pub fn from_file(filename: PathBuf) -> VmafData {
        let byte_lines = fs::read(filename).unwrap();
        let lines = std::str::from_utf8(&byte_lines).unwrap();

        serde_json::from_str(lines).unwrap()
    }

    pub fn to_vec(&self) -> Vec<f32> {
        let mut out: Vec<f32> = vec![];
        for val in self.frames.iter() {
            out.push(val.metrics.vmaf);
        }

        out
    }

    pub fn gen_pts(&self) -> Vec<(f32, f32)> {
        let mut pts: Vec<(f32, f32)> = Vec::new();

        for (idx, frame) in self.frames.iter().enumerate() {
            pts.push((idx.clone() as f32, frame.metrics.vmaf.clone()));
        }

        pts
    }
}

impl Display for VmafData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VMAF\n\tMin:\t\t\t{}\n\tMax:\t\t\t{}\n\tMean:\t\t\t{}\n\tHarmonic Mean:\t\t{}",
            self.pooled_metrics.vmaf.min,
            self.pooled_metrics.vmaf.max,
            self.pooled_metrics.vmaf.mean,
            self.pooled_metrics.vmaf.harmonic_mean,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_vmaf_json_data() {
        let byte_lines = fs::read("src/command/vmaf/sample/vmaf_stats_short.json").unwrap();
        let lines = std::str::from_utf8(&byte_lines).unwrap();

        let value: VmafData = serde_json::from_str(lines).unwrap();

        let exemplar = VmafData {
            version: "3.0.0".to_string(),
            fps: 9.59,
            frames: vec![
                VmafFrameData {
                    frameNum: 0,
                    metrics: VmafMetrics {
                        integer_motion2: 0.000000,
                        integer_motion: 0.000000,
                        integer_adm2: 0.991197,
                        integer_adm_scale0: 0.974915,
                        integer_adm_scale1: 0.978153,
                        integer_adm_scale2: 0.993203,
                        integer_adm_scale3: 0.997849,
                        integer_vif_scale0: 0.719183,
                        integer_vif_scale1: 0.964333,
                        integer_vif_scale2: 0.985399,
                        integer_vif_scale3: 0.992346,
                        vmaf: 94.141850,
                    },
                },
                VmafFrameData {
                    frameNum: 1,
                    metrics: VmafMetrics {
                        integer_motion2: 3.796119,
                        integer_motion: 3.796119,
                        integer_adm2: 0.986334,
                        integer_adm_scale0: 0.965558,
                        integer_adm_scale1: 0.966637,
                        integer_adm_scale2: 0.987297,
                        integer_adm_scale3: 0.996373,
                        integer_vif_scale0: 0.611066,
                        integer_vif_scale1: 0.975794,
                        integer_vif_scale2: 0.991188,
                        integer_vif_scale3: 0.995675,
                        vmaf: 98.548040,
                    },
                },
                VmafFrameData {
                    frameNum: 2,
                    metrics: VmafMetrics {
                        integer_motion2: 4.315013,
                        integer_motion: 4.315013,
                        integer_adm2: 0.991404,
                        integer_adm_scale0: 0.970170,
                        integer_adm_scale1: 0.978360,
                        integer_adm_scale2: 0.994236,
                        integer_adm_scale3: 0.998749,
                        integer_vif_scale0: 0.686090,
                        integer_vif_scale1: 0.988201,
                        integer_vif_scale2: 0.996071,
                        integer_vif_scale3: 0.998163,
                        vmaf: 100.000000,
                    },
                },
                VmafFrameData {
                    frameNum: 3,
                    metrics: VmafMetrics {
                        integer_motion2: 4.766777,
                        integer_motion: 4.766777,
                        integer_adm2: 0.979022,
                        integer_adm_scale0: 0.958306,
                        integer_adm_scale1: 0.952069,
                        integer_adm_scale2: 0.977558,
                        integer_adm_scale3: 0.992804,
                        integer_vif_scale0: 0.514936,
                        integer_vif_scale1: 0.963631,
                        integer_vif_scale2: 0.987109,
                        integer_vif_scale3: 0.992891,
                        vmaf: 97.722232,
                    },
                },
                VmafFrameData {
                    frameNum: 4,
                    metrics: VmafMetrics {
                        integer_motion2: 5.500895,
                        integer_motion: 5.500895,
                        integer_adm2: 0.992631,
                        integer_adm_scale0: 0.974205,
                        integer_adm_scale1: 0.981557,
                        integer_adm_scale2: 0.995222,
                        integer_adm_scale3: 0.998731,
                        integer_vif_scale0: 0.709727,
                        integer_vif_scale1: 0.990267,
                        integer_vif_scale2: 0.996725,
                        integer_vif_scale3: 0.998389,
                        vmaf: 100.000000,
                    },
                },
                VmafFrameData {
                    frameNum: 5,
                    metrics: VmafMetrics {
                        integer_motion2: 5.710850,
                        integer_motion: 5.710850,
                        integer_adm2: 0.979443,
                        integer_adm_scale0: 0.954544,
                        integer_adm_scale1: 0.951750,
                        integer_adm_scale2: 0.979627,
                        integer_adm_scale3: 0.993829,
                        integer_vif_scale0: 0.519958,
                        integer_vif_scale1: 0.963766,
                        integer_vif_scale2: 0.986752,
                        integer_vif_scale3: 0.992692,
                        vmaf: 98.871505,
                    },
                },
            ],
            pooled_metrics: VmafPooledMetrics {
                integer_motion2: VmafSummaryData {
                    min: 0.000000,
                    max: 6.477980,
                    mean: 4.064368,
                    harmonic_mean: 3.913283,
                },
                integer_motion: VmafSummaryData {
                    min: 0.000000,
                    max: 7.150653,
                    mean: 4.160725,
                    harmonic_mean: 4.004319,
                },
                integer_adm2: VmafSummaryData {
                    min: 0.977446,
                    max: 0.994818,
                    mean: 0.985168,
                    harmonic_mean: 0.985158,
                },
                integer_adm_scale0: VmafSummaryData {
                    min: 0.947397,
                    max: 0.980837,
                    mean: 0.962225,
                    harmonic_mean: 0.962197,
                },
                integer_adm_scale1: VmafSummaryData {
                    min: 0.942085,
                    max: 0.987743,
                    mean: 0.960708,
                    harmonic_mean: 0.960642,
                },
                integer_adm_scale2: VmafSummaryData {
                    min: 0.974982,
                    max: 0.997051,
                    mean: 0.986623,
                    harmonic_mean: 0.986609,
                },
                integer_adm_scale3: VmafSummaryData {
                    min: 0.991618,
                    max: 0.999606,
                    mean: 0.997189,
                    harmonic_mean: 0.997188,
                },
                integer_vif_scale0: VmafSummaryData {
                    min: 0.466131,
                    max: 0.792144,
                    mean: 0.570250,
                    harmonic_mean: 0.566741,
                },
                integer_vif_scale1: VmafSummaryData {
                    min: 0.952431,
                    max: 0.994077,
                    mean: 0.973837,
                    harmonic_mean: 0.973782,
                },
                integer_vif_scale2: VmafSummaryData {
                    min: 0.980234,
                    max: 0.998007,
                    mean: 0.992008,
                    harmonic_mean: 0.991999,
                },
                integer_vif_scale3: VmafSummaryData {
                    min: 0.989822,
                    max: 0.999108,
                    mean: 0.996108,
                    harmonic_mean: 0.996105,
                },
                vmaf: VmafSummaryData {
                    min: 94.141850,
                    max: 100.000000,
                    mean: 98.489315,
                    harmonic_mean: 98.474808,
                },
            },
            aggregate_metrics: VmafAggregateMetrics {},
        };
        assert_eq!(value, exemplar);
    }
}
