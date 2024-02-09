pub mod parser;

use crate::command::vmaf::parser::{
    VmafData, VmafFrameData, VmafMetrics, VmafPooledMetrics, VmafSummaryData,
};
use crate::{
    command::{
        args::{self, PixelFormat},
        PROGRESS_CHARS,
    },
    ffprobe, plot,
    process::FfmpegOut,
    stats::Stats,
    vmaf,
    vmaf::VmafOut,
};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::{path::PathBuf, time::Duration};
use tokio_stream::StreamExt;

/// Full VMAF score calculation, distorted file vs reference file.
/// Works with videos and images.
///
/// * Auto sets model version (4k or 1k) according to resolution.
/// * Auto sets `n_threads` to system threads.
/// * Auto upscales lower resolution videos to the model.
/// * Converts distorted & reference to appropriate format yuv streams before passing to vmaf.
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
    pub vmaf: args::Vmaf,
}

pub async fn vmaf(
    Args {
        reference,
        reference_vfilter,
        distorted,
        vmaf,
    }: Args,
) -> anyhow::Result<()> {
    let bar = ProgressBar::new(1).with_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan.bold} {elapsed_precise:.bold} {wide_bar:.cyan/blue} ({msg}eta {eta})")?
            .progress_chars(PROGRESS_CHARS)
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message("vmaf running, ");

    let dprobe = ffprobe::probe(&distorted);
    let dpix_fmt = dprobe.pixel_format().unwrap_or(PixelFormat::Yuv444p10le);
    let rprobe = ffprobe::probe(&reference);
    let rpix_fmt = rprobe.pixel_format().unwrap_or(PixelFormat::Yuv444p10le);
    let nframes = dprobe.nframes().or_else(|_| rprobe.nframes());
    if let Ok(nframes) = nframes {
        bar.set_length(nframes);
    }

    let mut logfile_name = distorted.clone();
    logfile_name.set_extension("json");
    let mut vmaf = vmaf::run(
        &rprobe,
        &dprobe,
        &vmaf.ffmpeg_lavfi(
            dprobe.resolution,
            dpix_fmt.max(rpix_fmt),
            reference_vfilter.as_deref(),
            Some(logfile_name.clone()),
        ),
    )?;
    // let mut vmaf_score = -1.0;
    while let Some(vmaf) = vmaf.next().await {
        match vmaf {
            VmafOut::Done(score) => {
                // vmaf_score = score;
                break;
            }
            VmafOut::Progress(FfmpegOut::Progress { frame, fps, .. }) => {
                if fps > 0.0 {
                    bar.set_message(format!("vmaf {fps} fps, "));
                }
                if nframes.is_ok() {
                    bar.set_position(frame);
                }
            }
            VmafOut::Progress(FfmpegOut::StreamSizes { .. }) => {}
            VmafOut::Err(e) => return Err(e),
        }
    }
    std::thread::sleep(std::time::Duration::new(1, 0));
    bar.finish();

    let vmaf_score = VmafData::from_file(logfile_name);
    let vmaf_stats = Stats::calc_stats(&vmaf_score.to_vec());

    println!("{vmaf_score}");
    println!("{vmaf_stats}");

    // let pts = vmaf_score.gen_pts();
    // let mut graph_name = distorted.clone();
    // graph_name.set_extension("png");
    // plot::plot(
    //     pts,
    //     &vmaf_stats.eff_min,
    //     &vmaf_stats.harmonic_mean,
    //     graph_name,
    // );

    Ok(())
}
