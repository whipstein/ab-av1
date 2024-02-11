mod err;

pub use err::Error;

use crate::{
    command::{
        args, bitrate_search::err::ensure_or_no_good_br,
        encoders::videotoolbox::VideotoolboxEncoder, encoders::Encoder, sample_encode,
        PROGRESS_CHARS,
    },
    console_ext::style,
    ffprobe,
    ffprobe::Ffprobe,
    float::TerseF32,
};
use clap::{ArgAction, Parser, ValueHint};
use console::style;
use err::ensure_other;
use indicatif::{HumanBytes, HumanDuration, ProgressBar, ProgressStyle};
use ordered_float::OrderedFloat;
use std::{
    cmp::Ordering,
    io::{self, IsTerminal},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

const BAR_LEN: u64 = 1_000_000_000;

/// Interpolated binary search using sample-encode to find the best crf
/// value delivering min-vmaf & max-encoded-percent.
///
/// Outputs:
/// * Best crf value
/// * Mean sample VMAF score
/// * Predicted full encode size
/// * Predicted full encode time
#[derive(Parser)]
#[clap(verbatim_doc_comment)]
#[group(skip)]
pub struct Args {
    #[clap(flatten)]
    pub args: VideotoolboxEncoder,

    /// Input video file.
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    pub input: PathBuf,

    /// Desired min VMAF score to deliver.
    #[arg(long, default_value_t = 95.0)]
    pub min_vmaf: f32,

    /// Maximum desired encoded size percentage of the input size.
    #[arg(long, default_value_t = 80.0)]
    pub max_encoded_percent: f32,

    /// Minimum (highest quality) crf value to try.
    #[arg(long, default_value_t = 100)]
    pub min_br: u32,

    /// Maximum (lowest quality) crf value to try.
    ///
    /// [default: 55, 46 for x264,x265, 255 for rav1e]
    #[arg(long)]
    pub max_br: Option<u32>,

    /// Keep searching until a crf is found no more than min_vmaf+0.05 or all
    /// possibilities have been attempted.
    ///
    /// By default the "higher vmaf tolerance" increases with each attempt (0.1, 0.2, 0.4 etc...).
    #[arg(long)]
    pub thorough: bool,

    /// Constant rate factor search increment precision.
    ///
    /// [default: 1.0, 0.1 for x264,x265,vp9]
    #[arg(long)]
    pub br_increment: Option<u32>,

    /// Enable sample-encode caching.
    #[arg(
        long,
        default_value_t = true,
        env = "AB_AV1_CACHE",
        action(ArgAction::Set)
    )]
    pub cache: bool,

    #[clap(flatten)]
    pub sample: args::Sample,

    #[clap(flatten)]
    pub vmaf: args::Vmaf,

    #[arg(skip)]
    pub quiet: bool,
}

pub async fn bitrate_search(mut args: Args) -> anyhow::Result<()> {
    let bar = ProgressBar::new(12).with_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan.bold} {elapsed_precise:.bold} {wide_bar:.cyan/blue} ({msg}eta {eta})")?
            .progress_chars(PROGRESS_CHARS)
    );

    let probe = ffprobe::probe(&args.input);
    let input_is_image = probe.is_image;
    args.sample.set_extension_from_input(&args.input, &probe);

    let best = run(&args, probe.into(), bar.clone()).await;
    bar.finish();
    let best = best?;

    // encode how-to hint + predictions
    eprintln!(
        "\n{} {}\n",
        style("Encode with:").dim(),
        style(args.args.encode_hint()).dim().italic(),
    );

    StdoutFormat::Human.print_result(&best, input_is_image);

    Ok(())
}

pub async fn run(
    Args {
        args,
        input,
        min_vmaf,
        max_encoded_percent,
        min_br,
        max_br,
        br_increment,
        thorough,
        sample,
        quiet,
        cache,
        vmaf,
    }: &Args,
    input_probe: Arc<Ffprobe>,
    bar: ProgressBar,
) -> Result<Sample, Error> {
    let max_br = max_br.unwrap_or_else(|| args.encoder.default_max_br());
    ensure_other!(*min_br < max_br, "Invalid --min-crf & --max-crf");

    let br_increment = br_increment
        .unwrap_or_else(|| args.encoder.default_br_increment())
        .max(100);

    let min_q = q_from_br_lin(*min_br, br_increment);
    let max_q = q_from_br_lin(max_br, br_increment);
    let mut q: f64 = (min_q + max_q) / 2.0;

    let mut args = sample_encode::Args {
        args: args.clone(),
        input: input.clone(),
        sample: sample.clone(),
        cache: *cache,
        stdout_format: sample_encode::StdoutFormat::Json,
        vmaf: vmaf.clone(),
    };

    bar.set_length(BAR_LEN);
    let sample_bar = ProgressBar::hidden();
    let mut br_attempts = Vec::new();

    for run in 1.. {
        // how much we're prepared to go higher than the min-vmaf
        let higher_tolerance = match thorough {
            true => 0.05,
            // increment 1.0 => +0.1, +0.2, +0.4, +0.8 ..
            // increment 0.1 => +0.1, +0.1, +0.1, +0.16 ..
            _ => (br_increment as f32 * 2_f32.powi(run as i32 - 1) * 0.1).max(0.1),
        };
        args.args.bitrate = Some(q.to_br(br_increment));
        bar.set_message(format!(
            "sampling bitrate {}k, ",
            args.args.bitrate.unwrap().to_string()
        ));

        // run sample encode
        let mut sample_task = tokio::task::spawn_local(sample_encode::run(
            args.clone(),
            input_probe.clone(),
            sample_bar.clone(),
        ));

        let sample_task = loop {
            match tokio::time::timeout(Duration::from_millis(100), &mut sample_task).await {
                Err(_) => {
                    let sample_progress = sample_bar.position() as f64
                        / sample_bar.length().unwrap_or(1).max(1) as f64;
                    bar.set_position(guess_progress(run, sample_progress, *thorough) as _);
                }
                Ok(o) => {
                    sample_bar.set_position(0);
                    break o;
                }
            }
        };

        // initial sample encoding results
        let sample = Sample {
            enc: sample_task??,
            br_increment,
            q,
        };
        let from_cache = sample.enc.from_cache;
        br_attempts.push(sample.clone());
        let sample_small_enough = sample.enc.encode_percent <= *max_encoded_percent as _;

        if sample.enc.vmaf > *min_vmaf {
            // Good Enough

            // is the encoding too big or using maximum bitrate?
            if sample_small_enough && sample.enc.vmaf < min_vmaf + higher_tolerance {
                return Ok(sample);
            }

            // set a new lower bound from existing encodings
            let l_bound = br_attempts
                .iter()
                .filter(|s| s.q < sample.q)
                .max_by_key(|s| OrderedFloat(s.q));

            match l_bound {
                Some(lower) if lower.q == sample.q + 1.0 => {
                    ensure_or_no_good_br!(sample_small_enough, sample);
                    return Ok(sample);
                }
                Some(lower) => {
                    q = vmaf_lerp_q(*min_vmaf, lower, &sample);
                }
                None if sample.q == min_q => {
                    ensure_or_no_good_br!(sample_small_enough, sample);
                    return Ok(sample);
                }
                None if run == 1 && sample.q + 1.0 < min_q => {
                    q = (sample.q + min_q) / 2.0;
                }
                None => q = min_q,
            };
        } else {
            // Not Good Enough

            // is the encoding too big or using maximum bitrate?
            if !sample_small_enough || sample.q == max_q {
                sample.print_attempt(&bar, *min_vmaf, *max_encoded_percent, *quiet, from_cache);
                ensure_or_no_good_br!(false, sample);
            }

            // set a new upper bound from existing encodings
            let u_bound = br_attempts
                .iter()
                .filter(|s| s.q > sample.q)
                .min_by_key(|s| OrderedFloat(s.q));

            match u_bound {
                Some(upper) if upper.q - 1.0 == sample.q => {
                    sample.print_attempt(&bar, *min_vmaf, *max_encoded_percent, *quiet, from_cache);
                    let lower_small_enough = upper.enc.encode_percent <= *max_encoded_percent as _;
                    ensure_or_no_good_br!(lower_small_enough, sample);
                    return Ok(upper.clone());
                }
                Some(upper) => {
                    q = vmaf_lerp_q(*min_vmaf, &sample, upper);
                }
                None if run == 1 && sample.q > max_q + 1.0 => {
                    q = (max_q + sample.q) / 2.0;
                }
                None => q = max_q,
            };
        }
        sample.print_attempt(&bar, *min_vmaf, *max_encoded_percent, *quiet, from_cache);
    }

    unreachable!();
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub enc: sample_encode::Output,
    pub br_increment: u32,
    pub q: f64,
}

impl Sample {
    pub fn br(&self) -> u32 {
        self.q.to_br(self.br_increment)
    }

    fn print_attempt(
        &self,
        bar: &ProgressBar,
        min_vmaf: f32,
        max_encoded_percent: f32,
        quiet: bool,
        from_cache: bool,
    ) {
        if quiet {
            return;
        }
        let br_label = style("- br").dim();
        let mut br = style(self.br());
        let vmaf_label = style("VMAF").dim();
        let mut vmaf = style(self.enc.vmaf);
        let mut percent = style!("{:.0}%", self.enc.encode_percent);
        let open = style("(").dim();
        let close = style(")").dim();
        let cache_msg = match from_cache {
            true => style(" (cache)").dim(),
            false => style(""),
        };

        if self.enc.vmaf < min_vmaf {
            br = br.red().bright();
            vmaf = vmaf.red().bright();
        }
        if self.enc.encode_percent > max_encoded_percent as _ {
            br = br.red().bright();
            percent = percent.red().bright();
        }

        let msg =
            format!("{br_label} {br} {vmaf_label} {vmaf:.2} {open}{percent}{close}{cache_msg}");
        if io::stderr().is_terminal() {
            bar.println(msg);
        } else {
            eprintln!("{msg}");
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum StdoutFormat {
    Human,
}

impl StdoutFormat {
    fn print_result(self, sample: &Sample, image: bool) {
        match self {
            Self::Human => {
                let br = style(sample.br()).bold().green();
                let enc = &sample.enc;
                let vmaf = style(enc.vmaf).bold().green();
                let size = style(HumanBytes(enc.predicted_encode_size)).bold().green();
                let percent = style!("{}%", enc.encode_percent.round()).bold().green();
                let time = style(HumanDuration(enc.predicted_encode_time)).bold();
                let enc_description = match image {
                    true => "image",
                    false => "video stream",
                };
                println!(
                    "br {br} VMAF {vmaf:.2} predicted {enc_description} size {size} ({percent}) taking {time}"
                );
            }
        }
    }
}

/// Produce a q value between given samples using vmaf score linear interpolation
/// so the output q value should produce the `min_vmaf`.
///
/// Note: `better_q` will be a numerically higher q value (higher quality),
///       `worse_q` a numerically lower q value (worse quality).
///
/// # Issues
/// Bitrate values do not linearly map to VMAF changes (or anything?) so this is a flawed method,
/// though it seems to work better than a binary search.
/// Perhaps a better approximation of a general br->vmaf model could be found.
/// This would be helpful particularly for small br-increments.
fn vmaf_lerp_q(min_vmaf: f32, worse_q: &Sample, better_q: &Sample) -> f64 {
    assert!(
        worse_q.enc.vmaf <= min_vmaf
            && worse_q.enc.vmaf < better_q.enc.vmaf
            && worse_q.q < better_q.q,
        "invalid vmaf_lerp_br usage: ({min_vmaf}, {worse_q:?}, {better_q:?})"
    );

    // let vmaf_diff = better_q.enc.vmaf - worse_q.enc.vmaf;
    // let vmaf_factor = (min_vmaf - worse_q.enc.vmaf) / vmaf_diff;

    // let q_diff = better_q.q - worse_q.q;
    // let lerp = better_q.q - q_diff * vmaf_factor as f64;
    let lerp = (worse_q.q * (better_q.enc.vmaf - min_vmaf) as f64
        + better_q.q * (min_vmaf - worse_q.enc.vmaf) as f64)
        / (better_q.enc.vmaf - worse_q.enc.vmaf) as f64;
    lerp.clamp(worse_q.q + 1.0, better_q.q - 1.0)
}

/// sample_progress: [0, 1]
fn guess_progress(run: usize, sample_progress: f64, thorough: bool) -> f64 {
    let total_runs_guess = match () {
        // Guess 6 iterations for a "thorough" search
        _ if thorough && run < 7 => 6.0,
        // Guess 4 iterations initially
        _ if run < 5 => 4.0,
        // Otherwise guess next will work
        _ => run as f64,
    };
    ((run - 1) as f64 + sample_progress) * BAR_LEN as f64 / total_runs_guess
}

/// Calculate "q" as a quality value integer multiple of bitrate.
///
/// * br=12500, inc=10 -> q=1250
/// * br=5000, inc=100 -> q=50
#[inline]
fn q_from_br(br: u32, br_increment: u32) -> u64 {
    (f64::from(br) / f64::from(br_increment)) as _
}

/// Ln Transform bitrate.
///
/// * br=12500 -> q=9.433483923290392
/// * br=5000 -> q=8.517193191416238
#[inline]
fn q_from_br_ln(br: u32, br_increment: u32) -> f64 {
    f64::from(br).ln() as _
}

/// Sqrt Transform bitrate.
///
/// * br=12500 -> q=111.80339887498948
/// * br=5000 -> q=70.71067811865476
#[inline]
fn q_from_br_sqrt(br: u32, br_increment: u32) -> f64 {
    f64::from(br).sqrt() as _
}

/// Linear Transform bitrate.
///
/// * br=12500 -> q=12500
/// * br=5000 -> q=5000
#[inline]
fn q_from_br_lin(br: u32, br_increment: u32) -> f64 {
    (f64::from(br) / f64::from(br_increment)) as _
}

/// No Transform bitrate.
///
/// * br=12500 -> q=12500
/// * br=5000 -> q=5000
#[inline]
fn q_from_br_no(br: u32, br_increment: u32) -> f64 {
    f64::from(br) as _
}

trait QualityValue {
    fn to_br(self, br_increment: u32) -> u32;
}
impl QualityValue for u64 {
    #[inline]
    fn to_br(self, br_increment: u32) -> u32 {
        ((self as u64) * u64::from(br_increment)) as _
    }
}

impl QualityValue for f64 {
    // #[inline]
    // fn to_br(self, br_increment: u32) -> u32 {
    //     self.exp().round() as _
    // }
    // #[inline]
    // fn to_br(self, br_increment: u32) -> u32 {
    //     self.powi(2).round() as _
    // }
    #[inline]
    fn to_br(self, br_increment: u32) -> u32 {
        (self * br_increment as f64).round() as _
    }
    // #[inline]
    // fn to_br(self, br_increment: u32) -> u32 {
    //     self.round() as _
    // }
}

mod test {
    use super::*;

    #[test]
    fn q_br_lin_conversions() {
        assert_eq!(q_from_br(12500, 10), 1250);
        assert_eq!(q_from_br(5000, 100), 50);
    }

    #[test]
    fn q_br_ln_conversions() {
        assert_eq!(q_from_br_ln(12500, 10), 9.433483923290392);
        assert_eq!(q_from_br_ln(5000, 10), 8.517193191416238);
    }

    #[test]
    fn q_br_sqrt_conversions() {
        assert_eq!(q_from_br_sqrt(12500, 10), 111.80339887498948);
        assert_eq!(q_from_br_sqrt(5000, 10), 70.71067811865476);
    }
}
