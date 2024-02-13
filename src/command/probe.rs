use crate::{
    command::{
        args::{self},
        // encoders::svtav1::SvtEncoder,
        encoders::videotoolbox::VideotoolboxEncoder,
        encoders::{Encoder, EncoderString},
        SmallDuration,
        PROGRESS_CHARS,
    },
    console_ext::style,
    ffmpeg,
    ffmpeg::FfmpegEncodeArgs,
    ffprobe::{self, Ffprobe},
    process::FfmpegOut,
    temporary::{self, TempKind},
};
use clap::{Parser, ValueHint};
use console::style;
use indicatif::{HumanBytes, ProgressBar, ProgressStyle};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::fs;
use tokio_stream::StreamExt;

/// Invoke ffmpeg to encode a video or image.
#[derive(Parser)]
#[group(skip)]
pub struct Args {
    /// Input video file.
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    pub input: PathBuf,
}

pub async fn probe(args: Args) -> anyhow::Result<()> {
    let bar = ProgressBar::new(1).with_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan.bold} {elapsed_precise:.bold} {wide_bar:.cyan/blue} ({msg}eta {eta})")?
            .progress_chars(PROGRESS_CHARS)
    );
    bar.enable_steady_tick(Duration::from_millis(100));

    _ = ffprobe::probe(&args.input, true);
    Ok(())
}
