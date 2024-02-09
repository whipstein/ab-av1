mod parser;

use crate::command::ssim::parser::{SsimData, SsimFrameData};
use crate::{
    command::{
        args::{self, PixelFormat},
        ssim::parser::parse_ssim_stdout_line,
        PROGRESS_CHARS,
    },
    ffprobe,
    process::FfmpegOut,
    ssim,
    ssim::SsimOut,
};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use nom_bufreader::async_bufreader::BufReader;
use nom_bufreader::{Error, Parse};
use std::io::BufRead;
use std::{fs, fs::File, path::PathBuf, time::Duration};
use tokio_stream::StreamExt;

use self::parser::parse_input;

/// Full SSIM score calculation, distorted file vs reference file.
/// Works with videos and images.
///
/// * Auto upscales lower resolution videos to the model.
/// * Converts distorted & reference to appropriate format yuv streams before passing to ssim.
#[derive(Parser)]
#[clap(verbatim_doc_comment)]
#[group(skip)]
pub struct Args {
    /// Reference video file.
    #[arg(short, long)]
    pub reference: PathBuf,

    /// Ffmpeg video filter applied to the reference before analysis.
    /// E.g. --reference-vfilter "scale=1280:-1,fps=24".
    #[arg(long)]
    pub reference_vfilter: Option<String>,

    /// Re-encoded/distorted video file.
    #[arg(short, long)]
    pub distorted: PathBuf,

    #[clap(flatten)]
    pub ssim: args::Ssim,
}

pub async fn ssim<'a>(
    Args {
        reference,
        reference_vfilter,
        distorted,
        ssim,
    }: Args,
) -> anyhow::Result<()> {
    let bar = ProgressBar::new(1).with_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan.bold} {elapsed_precise:.bold} {wide_bar:.cyan/blue} ({msg}eta {eta})")?
            .progress_chars(PROGRESS_CHARS)
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message("ssim running, ");

    let dprobe = ffprobe::probe(&distorted);
    let dpix_fmt = dprobe.pixel_format().unwrap_or(PixelFormat::Yuv444p10le);
    let rprobe = ffprobe::probe(&reference);
    let rpix_fmt = rprobe.pixel_format().unwrap_or(PixelFormat::Yuv444p10le);
    let nframes = dprobe.nframes().or_else(|_| rprobe.nframes());
    if let Ok(nframes) = nframes {
        bar.set_length(nframes);
    }

    let mut ssim = ssim::run(
        &reference,
        &distorted,
        &ssim.ffmpeg_lavfi(
            dprobe.resolution,
            dpix_fmt.max(rpix_fmt),
            reference_vfilter.as_deref(),
        ),
    )?;
    let mut ssim_score = -1.0;
    while let Some(ssim) = ssim.next().await {
        match ssim {
            SsimOut::Done(score) => {
                ssim_score = score;
                break;
            }
            SsimOut::Progress(FfmpegOut::Progress { frame, fps, .. }) => {
                if fps > 0.0 {
                    bar.set_message(format!("ssim {fps} fps, "));
                }
                if nframes.is_ok() {
                    bar.set_position(frame);
                }
            }
            SsimOut::Progress(FfmpegOut::StreamSizes { .. }) => {}
            SsimOut::Err(e) => return Err(e),
        }
    }
    bar.finish();

    let byte_lines = fs::read("ssim_stats.log").unwrap();
    let lines = std::str::from_utf8(&byte_lines).unwrap();
    let lines = parse_input(lines.as_bytes());
    let data = SsimData::from_vec(&lines);

    println!("{}", data);
    Ok(())
}
