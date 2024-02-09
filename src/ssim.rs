//! ssim logic
use crate::process::{exit_ok_stderr, Chunks, CommandExt, FfmpegOut};
use anyhow::Context;
use std::path::Path;
use tokio::process::Command;
use tokio_process_stream::{Item, ProcessChunkStream};
use tokio_stream::{Stream, StreamExt};

/// Calculate SSIM score by converting the original first to yuv.
/// This can produce more accurate results than testing directly from original source.
pub fn run(
    reference: &Path,
    distorted: &Path,
    filter_complex: &str,
) -> anyhow::Result<impl Stream<Item = SsimOut>> {
    let ssim: ProcessChunkStream = Command::new("ffmpeg")
        .kill_on_drop(true)
        .arg2("-r", "24")
        .arg2("-i", distorted)
        .arg2("-r", "24")
        .arg2("-i", reference)
        .arg2("-filter_complex", filter_complex)
        .arg2("-f", "null")
        .arg("-")
        .try_into()
        .context("ffmpeg ssim")?;

    let mut chunks = Chunks::default();
    let ssim = ssim.filter_map(move |item| match item {
        Item::Stderr(chunk) => SsimOut::try_from_chunk(&chunk, &mut chunks),
        Item::Stdout(_) => None,
        Item::Done(code) => SsimOut::ignore_ok(exit_ok_stderr("ffmpeg ssim", code, &chunks)),
    });

    Ok(ssim)
}

#[derive(Debug)]
pub enum SsimOut {
    Progress(FfmpegOut),
    Done(f32),
    Err(anyhow::Error),
}

impl SsimOut {
    fn ignore_ok<T>(result: anyhow::Result<T>) -> Option<Self> {
        match result {
            Ok(_) => None,
            Err(err) => Some(Self::Err(err)),
        }
    }

    fn try_from_chunk(chunk: &[u8], chunks: &mut Chunks) -> Option<Self> {
        chunks.push(chunk);
        let line = chunks.last_line();

        if let Some(idx) = line.find("SSIM Y: ") {
            return Some(Self::Done(
                line[idx + "SSIM Y: ".len()..].trim().parse().ok()?,
            ));
        }
        if let Some(progress) = FfmpegOut::try_parse(line) {
            return Some(Self::Progress(progress));
        }
        None
    }
}
