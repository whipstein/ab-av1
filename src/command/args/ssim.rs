use crate::command::args::PixelFormat;
use anyhow::Context;
use clap::Parser;
use std::{borrow::Cow, fmt::Display, sync::Arc};

/// Common ssim options.
#[derive(Parser, Clone, Hash)]
pub struct Ssim {
    /// Additional ssim arg(s). E.g. --ssim stats_file=stats.log
    ///
    /// Also see https://ffmpeg.org/ffmpeg-filters.html#ssim.
    #[arg(long = "ssim", value_parser = parse_ssim_arg)]
    pub ssim_args: Vec<Arc<str>>,

    /// Video resolution scale to use in VMAF analysis. If set, video streams will be bicupic
    /// scaled to this width during VMAF analysis. `auto` (default) automatically sets
    /// based on the model and input video resolution. `none` disables any scaling.
    /// `WxH` format may be used to specify custom scaling, e.g. `1920x1080`.
    ///
    /// auto behaviour:
    /// * 1k model (default for resolutions <= 2560x1440) if width and height
    ///   are less than 1728 & 972 respectively upscale to 1080p. Otherwise no scaling.
    /// * 4k model (default for resolutions > 2560x1440) if width and height
    ///   are less than 3456 & 1944 respectively upscale to 4k. Otherwise no scaling.
    ///
    /// Scaling happens after any input/reference vfilters.
    #[arg(long, default_value_t = SsimScale::Auto, value_parser = parse_ssim_scale)]
    pub ssim_scale: SsimScale,
}

fn parse_ssim_arg(arg: &str) -> anyhow::Result<Arc<str>> {
    Ok(arg.to_owned().into())
}

impl Ssim {
    // pub fn is_default(&self) -> bool {
    //     self.ssim_args.is_empty()
    // }

    /// Returns ffmpeg `filter_complex`/`lavfi` value for calculating vmaf.
    pub fn ffmpeg_lavfi(
        &self,
        distorted_res: Option<(u32, u32)>,
        pix_fmt: PixelFormat,
        ref_vfilter: Option<&str>,
    ) -> String {
        let args = self.ssim_args.clone();
        let mut lavfi = args.join(":");
        // if self.is_default() {
        //     lavfi.insert_str(0, "ssim");
        // } else {
        //     lavfi.insert_str(0, "ssim=");
        // }
        lavfi.insert_str(0, "ssim=stats_file=ssim_stats.log");

        let mut model = SsimModel::from_args(&args);
        if let (None, Some((w, h))) = (model, distorted_res) {
            if w > 2560 && h > 1440 {
                // for >2k resoultions use 4k model
                // lavfi.push_str(":model=version=vmaf_4k_v0.6.1");
                model = Some(SsimModel::Ssim4K);
            }
        }

        let ref_vf: Cow<_> = match ref_vfilter {
            None => "".into(),
            Some(vf) if vf.ends_with(',') => vf.into(),
            Some(vf) => format!("{vf},").into(),
        };

        // prefix:
        // * Add reference-vfilter if any
        // * convert both streams to common pixel format
        // * scale to vmaf width if necessary
        // * sync presentation timestamp
        let prefix = if let Some((w, h)) = self.vf_scale(model.unwrap_or_default(), distorted_res) {
            format!(
                "[0:v]format={pix_fmt},scale={w}:{h}:flags=bicubic,setpts=PTS-STARTPTS[dis];\
                 [1:v]format={pix_fmt},{ref_vf}scale={w}:{h}:flags=bicubic,setpts=PTS-STARTPTS[ref];[dis][ref]"
            )
        } else {
            format!(
                "[0:v]format={pix_fmt},setpts=PTS-STARTPTS[dis];\
                 [1:v]format={pix_fmt},{ref_vf}setpts=PTS-STARTPTS[ref];[dis][ref]"
            )
        };

        lavfi.insert_str(0, &prefix);
        lavfi
    }

    fn vf_scale(&self, model: SsimModel, distorted_res: Option<(u32, u32)>) -> Option<(i32, i32)> {
        match (self.ssim_scale, distorted_res) {
            (SsimScale::Auto, Some((w, h))) => match model {
                // upscale small resolutions to 1k for use with the 1k model
                SsimModel::Ssim1K if w < 1728 && h < 972 => {
                    Some(minimally_scale((w, h), (1920, 1080)))
                }
                // upscale small resolutions to 4k for use with the 4k model
                SsimModel::Ssim4K if w < 3456 && h < 1944 => {
                    Some(minimally_scale((w, h), (3840, 2160)))
                }
                _ => None,
            },
            (SsimScale::Custom { width, height }, Some((w, h))) => {
                Some(minimally_scale((w, h), (width, height)))
            }
            (SsimScale::Custom { width, height }, None) => Some((width as _, height as _)),
            _ => None,
        }
    }
}

/// Return the smallest ffmpeg vf `(w, h)` scale values so that at least one of the
/// `target_w` or `target_h` bounds are met.
fn minimally_scale((from_w, from_h): (u32, u32), (target_w, target_h): (u32, u32)) -> (i32, i32) {
    let w_factor = from_w as f64 / target_w as f64;
    let h_factor = from_h as f64 / target_h as f64;
    if h_factor > w_factor {
        (-1, target_h as _) // scale vertically
    } else {
        (target_w as _, -1) // scale horizontally
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SsimScale {
    None,
    Auto,
    Custom { width: u32, height: u32 },
}

fn parse_ssim_scale(vs: &str) -> anyhow::Result<SsimScale> {
    const ERR: &str = "ssim-scale must be 'none', 'auto' or WxH format e.g. '1920x1080'";
    match vs {
        "none" => Ok(SsimScale::None),
        "auto" => Ok(SsimScale::Auto),
        _ => {
            let (w, h) = vs.split_once('x').context(ERR)?;
            let (width, height) = (w.parse().context(ERR)?, h.parse().context(ERR)?);
            Ok(SsimScale::Custom { width, height })
        }
    }
}

impl Display for SsimScale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => "none".fmt(f),
            Self::Auto => "auto".fmt(f),
            Self::Custom { width, height } => write!(f, "{width}x{height}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SsimModel {
    /// Default 1080p model.
    Ssim1K,
    /// 4k model.
    Ssim4K,
    /// Some other user specified model.
    Custom,
}

impl Default for SsimModel {
    fn default() -> Self {
        Self::Ssim1K
    }
}

impl SsimModel {
    fn from_args(args: &[Arc<str>]) -> Option<Self> {
        let mut using_custom_model: Vec<_> = args.iter().filter(|v| v.contains("model")).collect();

        match using_custom_model.len() {
            0 => None,
            1 => Some(match using_custom_model.remove(0) {
                v if v.ends_with("=1080") => Self::Ssim1K,
                v if v.ends_with("=4k") => Self::Ssim4K,
                _ => Self::Custom,
            }),
            _ => Some(Self::Custom),
        }
    }
}

mod test {
    use super::*;

    #[test]
    fn ssim_lavfi() {
        let ssim = Ssim {
            ssim_args: vec![],
            ssim_scale: SsimScale::Auto,
        };
        assert_eq!(
            ssim.ffmpeg_lavfi(None, PixelFormat::Yuv420p, Some("scale=1280:-1,fps=24")),
            "[0:v]format=yuv420p,setpts=PTS-STARTPTS[dis];\
         [1:v]format=yuv420p,scale=1280:-1,fps=24,setpts=PTS-STARTPTS[ref];\
         [dis][ref]ssim=stats_file=ssim_stats.log"
        );
    }

    #[test]
    fn ssim_lavfi_default() {
        let ssim = Ssim {
            ssim_args: vec![],
            ssim_scale: SsimScale::Auto,
        };
        let expected = format!(
            "[0:v]format=yuv420p10le,setpts=PTS-STARTPTS[dis];\
         [1:v]format=yuv420p10le,setpts=PTS-STARTPTS[ref];\
         [dis][ref]ssim=stats_file=ssim_stats.log"
        );
        assert_eq!(
            ssim.ffmpeg_lavfi(None, PixelFormat::Yuv420p10le, None),
            expected
        );
    }

    #[test]
    fn ssim_lavfi_include_stats_file() {
        let ssim = Ssim {
            ssim_args: vec!["stats_file=output.log".into()],
            ssim_scale: SsimScale::Auto,
        };
        let expected = format!(
            "[0:v]format=yuv420p,setpts=PTS-STARTPTS[dis];\
         [1:v]format=yuv420p,setpts=PTS-STARTPTS[ref];\
         [dis][ref]ssim=stats_file=output.log"
        );
        assert_eq!(
            ssim.ffmpeg_lavfi(None, PixelFormat::Yuv420p, None),
            expected
        );
    }

    /// Low resolution videos should be upscaled to 1080p
    #[test]
    fn ssim_lavfi_small_width() {
        let ssim = Ssim {
            ssim_args: vec![],
            ssim_scale: SsimScale::Auto,
        };
        assert_eq!(
            ssim.ffmpeg_lavfi(Some((1280, 720)), PixelFormat::Yuv420p, None),
            "[0:v]format=yuv420p,scale=1920:-1:flags=bicubic,setpts=PTS-STARTPTS[dis];\
         [1:v]format=yuv420p,scale=1920:-1:flags=bicubic,setpts=PTS-STARTPTS[ref];\
         [dis][ref]ssim=stats_file=ssim_stats.log"
        );
    }

    /// 4k videos should use 4k model
    #[test]
    fn ssim_lavfi_4k() {
        let ssim = Ssim {
            ssim_args: vec![],
            ssim_scale: SsimScale::Auto,
        };
        assert_eq!(
            ssim.ffmpeg_lavfi(Some((3840, 2160)), PixelFormat::Yuv420p, None),
            "[0:v]format=yuv420p,setpts=PTS-STARTPTS[dis];\
         [1:v]format=yuv420p,setpts=PTS-STARTPTS[ref];\
         [dis][ref]ssim=stats_file=ssim_stats.log"
        );
    }

    /// >2k videos should be upscaled to 4k & use 4k model
    #[test]
    fn ssim_lavfi_3k_upscale_to_4k() {
        let ssim = Ssim {
            ssim_args: vec![],
            ssim_scale: SsimScale::Auto,
        };
        assert_eq!(
            ssim.ffmpeg_lavfi(Some((3008, 1692)), PixelFormat::Yuv420p, None),
            "[0:v]format=yuv420p,scale=3840:-1:flags=bicubic,setpts=PTS-STARTPTS[dis];\
         [1:v]format=yuv420p,scale=3840:-1:flags=bicubic,setpts=PTS-STARTPTS[ref];\
         [dis][ref]ssim=stats_file=ssim_stats.log"
        );
    }

    #[test]
    fn ssim_lavfi_custom_model_and_width() {
        let ssim = Ssim {
            ssim_args: vec![],
            // if specified just do it
            ssim_scale: SsimScale::Custom {
                width: 123,
                height: 720,
            },
        };
        assert_eq!(
            ssim.ffmpeg_lavfi(Some((1280, 720)), PixelFormat::Yuv420p, None),
            "[0:v]format=yuv420p,scale=123:-1:flags=bicubic,setpts=PTS-STARTPTS[dis];\
         [1:v]format=yuv420p,scale=123:-1:flags=bicubic,setpts=PTS-STARTPTS[ref];\
         [dis][ref]ssim=stats_file=ssim_stats.log"
        );
    }

    #[test]
    fn ssim_lavfi_1080p() {
        let ssim = Ssim {
            ssim_args: vec![],
            ssim_scale: SsimScale::Auto,
        };
        assert_eq!(
            ssim.ffmpeg_lavfi(Some((1920, 1080)), PixelFormat::Yuv420p, None),
            "[0:v]format=yuv420p,setpts=PTS-STARTPTS[dis];\
         [1:v]format=yuv420p,setpts=PTS-STARTPTS[ref];\
         [dis][ref]ssim=stats_file=ssim_stats.log"
        );
    }
}
