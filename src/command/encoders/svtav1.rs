use crate::{
    command::encoders::{Encoder, EncoderString, KeyInterval, PixelFormat, Preset},
    ffmpeg::FfmpegEncodeArgs,
    ffprobe::{Ffprobe, ProbeError},
    float::TerseF32,
};
use anyhow::ensure;
use clap::{Parser, ValueHint};
use std::{
    collections::HashMap,
    fmt::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

/// Common svt-av1/ffmpeg input encoding arguments.
#[derive(Parser, Clone, Debug)]
pub struct SvtEncoder {
    /// Encoder override. See https://ffmpeg.org/ffmpeg-all.html#toc-Video-Encoders.
    ///
    /// [possible values: libsvtav1, libx264, libx265, libvpx-vp9, ...]
    #[arg(value_enum, short, long, default_value = "libsvtav1")]
    pub encoder: EncoderString,

    /// Encoded file pre-extension.
    #[arg(long, default_value = "av1")]
    pub ext: String,

    /// Ffmpeg video filter applied to the input before av1 encoding.
    /// E.g. --vfilter "scale=1280:-1,fps=24".
    ///
    /// See https://ffmpeg.org/ffmpeg-filters.html#Video-Filters
    #[arg(long)]
    pub vfilter: Option<String>,

    /// Pixel format. svt-av1 default yuv420p10le.
    #[arg(value_enum, long)]
    pub pix_format: Option<PixelFormat>,

    /// Encoder preset (0-13).
    /// Higher presets means faster encodes, but with a quality tradeoff.
    ///
    /// For some ffmpeg encoders a word may be used, e.g. "fast".
    /// libaom-av1 preset is mapped to equivalent -cpu-used argument.
    ///
    /// [svt-av1 default: 8]
    #[arg(long)]
    pub preset: Option<Preset>,

    /// Encoder crf (0-50).
    /// Higher presets means faster encodes, but with a quality tradeoff.
    ///
    /// For some ffmpeg encoders a word may be used, e.g. "fast".
    /// libaom-av1 preset is mapped to equivalent -cpu-used argument.
    ///
    /// [svt-av1 default: 8]
    #[arg(long)]
    pub crf: f32,

    /// Interval between keyframes. Can be specified as a number of frames, or a duration.
    /// E.g. "300" or "10s". Defaults to 10s if the input duration is over 3m.
    ///
    /// Longer intervals can give better compression but make seeking more coarse.
    /// Durations will be converted to frames using the input fps.
    ///
    /// Works on svt-av1 & most ffmpeg encoders set with --encoder.
    #[arg(long)]
    pub keyint: Option<KeyInterval>,

    /// Svt-av1 scene change detection, inserts keyframes at scene changes.
    /// Defaults on if using default keyint & the input duration is over 3m. Otherwise off.
    #[arg(long)]
    pub scd: Option<bool>,

    /// Additional svt-av1 arg(s). E.g. --svt mbr=2000 --svt film-grain=8
    ///
    /// See https://gitlab.com/AOMediaCodec/SVT-AV1/-/blob/master/Docs/svt-av1_encoder_user_guide.md#options
    #[arg(long = "svt", value_parser = parse_svt_arg)]
    pub svt_args: Vec<Arc<str>>,

    /// Additional ffmpeg encoder arg(s). E.g. `--enc x265-params=lossless=1`
    /// These are added as ffmpeg output file options.
    ///
    /// The first '=' symbol will be used to infer that this is an option with a value.
    /// Passed to ffmpeg like "x265-params=lossless=1" -> ['-x265-params', 'lossless=1']
    #[arg(long = "enc", allow_hyphen_values = true, value_parser = parse_enc_arg)]
    pub enc_args: Vec<String>,

    /// Additional ffmpeg input encoder arg(s). E.g. `--enc-input r=1`
    /// These are added as ffmpeg input file options.
    ///
    /// See --enc docs.
    #[arg(long = "enc-input", allow_hyphen_values = true, value_parser = parse_enc_arg)]
    pub enc_input_args: Vec<String>,
}

impl Encoder for SvtEncoder {
    fn encode_hint(&self) -> String {
        let Self {
            encoder,
            ext,
            vfilter,
            preset,
            crf,
            pix_format,
            keyint,
            scd,
            svt_args,
            enc_args,
            enc_input_args,
        } = self;

        let mut hint = "ab-av1 encode".to_owned();

        let vcodec = encoder.as_str();
        if vcodec != "libsvtav1" {
            write!(hint, " -e {vcodec}").unwrap();
        }
        write!(hint, " -i <INPUT> --crf {}", TerseF32(*crf)).unwrap();

        if let Some(preset) = preset {
            write!(hint, " --preset {preset}").unwrap();
        }
        if let Some(keyint) = keyint {
            write!(hint, " --keyint {keyint}").unwrap();
        }
        if let Some(scd) = scd {
            write!(hint, " --scd {scd}").unwrap();
        }
        if let Some(pix_fmt) = pix_format {
            write!(hint, " --pix-format {pix_fmt}").unwrap();
        }
        if let Some(filter) = vfilter {
            write!(hint, " --vfilter {filter:?}").unwrap();
        }
        for arg in svt_args {
            write!(hint, " --svt {arg}").unwrap();
        }
        for arg in enc_input_args {
            let arg = arg.trim_start_matches('-');
            write!(hint, " --enc-input {arg}").unwrap();
        }
        for arg in enc_args {
            let arg = arg.trim_start_matches('-');
            write!(hint, " --enc {arg}").unwrap();
        }

        hint
    }

    fn keyint(&self, probe: &Ffprobe) -> anyhow::Result<Option<i32>> {
        const KEYINT_DEFAULT_INPUT_MIN: Duration = Duration::from_secs(60 * 3);
        const KEYINT_DEFAULT: Duration = Duration::from_secs(10);

        let filter_fps = self
            .vfilter
            .as_deref()
            .and_then(super::try_parse_fps_vfilter);
        Ok(
            match (self.keyint, &probe.duration, &probe.fps, filter_fps) {
                // use the filter-fps if used, otherwise the input fps
                (Some(ki), .., Some(fps)) => Some(ki.keyint_number(Ok(fps))?),
                (Some(ki), _, fps, None) => Some(ki.keyint_number(fps.clone())?),
                (None, Ok(duration), _, Some(fps)) if *duration >= KEYINT_DEFAULT_INPUT_MIN => {
                    Some(KeyInterval::Duration(KEYINT_DEFAULT).keyint_number(Ok(fps))?)
                }
                (None, Ok(duration), Ok(fps), None) if *duration >= KEYINT_DEFAULT_INPUT_MIN => {
                    Some(KeyInterval::Duration(KEYINT_DEFAULT).keyint_number(Ok(*fps))?)
                }
                _ => None,
            },
        )
    }
}

fn parse_svt_arg(arg: &str) -> anyhow::Result<Arc<str>> {
    let arg = arg.trim_start_matches('-').to_owned();

    for deny in ["crf", "preset", "keyint", "scd", "input-depth"] {
        ensure!(!arg.starts_with(deny), "'{deny}' cannot be used here");
    }

    Ok(arg.into())
}

fn parse_enc_arg(arg: &str) -> anyhow::Result<String> {
    let mut arg = arg.to_owned();
    if !arg.starts_with('-') {
        arg.insert(0, '-');
    }

    ensure!(
        !arg.starts_with("-svtav1-params"),
        "'svtav1-params' cannot be set here, use `--svt`"
    );

    Ok(arg)
}

// mod test {
//     use super::*;

//     /// Should use keyint & scd defaults for >3m inputs.
//     #[test]
//     fn svtav1_to_ffmpeg_args_default_over_3m() {
//         let enc = SvtEncoder {
//             encoder: EncoderString("libsvtav1".into()),
//             ext: "av1".into(),
//             input: "vid.mp4".into(),
//             vfilter: Some("scale=320:-1,fps=film".into()),
//             preset: None,
//             crf: 32.0,
//             pix_format: None,
//             keyint: None,
//             scd: None,
//             svt_args: vec!["film-grain=30".into()],
//             enc_args: <_>::default(),
//             enc_input_args: <_>::default(),
//         };

//         let probe = Ffprobe {
//             duration: Ok(Duration::from_secs(300)),
//             has_audio: true,
//             max_audio_channels: None,
//             fps: Ok(30.0),
//             resolution: Some((1280, 720)),
//             is_image: false,
//             pix_fmt: None,
//         };

//         let FfmpegEncodeArgs {
//             input,
//             output,
//             vcodec,
//             vfilter,
//             pix_fmt,
//             output_args,
//             input_args,
//             video_only,
//         } = enc.to_ffmpeg_args(&probe).expect("to_ffmpeg_args");

//         assert_eq!(&*vcodec, "libsvtav1");
//         assert_eq!(input, enc.input);
//         assert_eq!(vfilter, Some("scale=320:-1,fps=film"));
//         assert_eq!(pix_fmt, PixelFormat::Yuv420p10le);
//         assert!(!video_only);

//         assert!(
//             output_args
//                 .windows(2)
//                 .any(|w| w[0].as_str() == "-g" && w[1].as_str() == "240"),
//             "expected -g in {output_args:?}"
//         );
//         let svtargs_idx = output_args
//             .iter()
//             .position(|a| a.as_str() == "-svtav1-params")
//             .expect("missing -svtav1-params");
//         let svtargs = output_args
//             .get(svtargs_idx + 1)
//             .expect("missing -svtav1-params value")
//             .as_str();
//         assert_eq!(svtargs, "scd=1:film-grain=30");
//         assert!(input_args.is_empty());
//     }

//     #[test]
//     fn svtav1_to_ffmpeg_args_default_under_3m() {
//         let enc = SvtEncoder {
//             encoder: EncoderString("libsvtav1".into()),
//             ext: "av1".into(),
//             input: "vid.mp4".into(),
//             vfilter: None,
//             preset: Some(Preset::Number(7)),
//             crf: 32.0,
//             pix_format: Some(PixelFormat::Yuv420p),
//             keyint: None,
//             scd: None,
//             svt_args: vec![],
//             enc_args: <_>::default(),
//             enc_input_args: <_>::default(),
//         };

//         let probe = Ffprobe {
//             duration: Ok(Duration::from_secs(179)),
//             has_audio: true,
//             max_audio_channels: None,
//             fps: Ok(24.0),
//             resolution: Some((1280, 720)),
//             is_image: false,
//             pix_fmt: None,
//         };

//         let FfmpegEncodeArgs {
//             input,
//             output,
//             vcodec,
//             vfilter,
//             pix_fmt,
//             output_args,
//             input_args,
//             video_only,
//         } = enc.to_ffmpeg_args(&probe).expect("to_ffmpeg_args");

//         assert_eq!(&*vcodec, "libsvtav1");
//         assert_eq!(input, enc.input);
//         assert_eq!(vfilter, None);
//         assert_eq!(pix_fmt, PixelFormat::Yuv420p);
//         assert!(!video_only);

//         assert!(
//             !output_args.iter().any(|a| a.as_str() == "-g"),
//             "unexpected -g in {output_args:?}"
//         );
//         let svtargs_idx = output_args
//             .iter()
//             .position(|a| a.as_str() == "-svtav1-params")
//             .expect("missing -svtav1-params");
//         let svtargs = output_args
//             .get(svtargs_idx + 1)
//             .expect("missing -svtav1-params value")
//             .as_str();
//         assert_eq!(svtargs, "scd=0");
//         assert!(input_args.is_empty());
//     }
// }
