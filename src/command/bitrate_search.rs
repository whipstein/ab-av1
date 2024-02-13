mod err;

pub use err::Error;
use futures::io::LineWriter;

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

/// Interpolated binary search using sample-encode to find the best bitrate
/// value delivering min-vmaf & max-encoded-percent.
///
/// Outputs:
/// * Best bitrate value
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

    let probe = ffprobe::probe(&args.input, false);
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
        .max(1);

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

    let mut sample = Sample::new(
        sample_encode::Output::new(),
        *min_br,
        max_br,
        br_increment,
        Transform::Linear,
    );

    for run in 1.. {
        // how much we're prepared to go higher than the min-vmaf
        let higher_tolerance = match thorough {
            true => 0.05,
            // increment 1.0 => +0.1, +0.2, +0.4, +0.8 ..
            // increment 0.1 => +0.1, +0.1, +0.1, +0.16 ..
            _ => (br_increment as f32 * 2_f32.powi(run as i32 - 1) * 0.1).max(0.1),
        };

        args.args.bitrate = Some(sample.val);
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

        // load sample encoding results
        sample.enc = sample_task??;

        let from_cache = sample.enc.from_cache;
        br_attempts.push(sample.clone());
        let sample_small_enough = sample.enc.encode_percent <= *max_encoded_percent as _;

        sample.val_to_prev();
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
                    sample.vmaf_lerp_q(*min_vmaf, Some(lower), None);
                }
                None if sample.q == sample.min_q => {
                    ensure_or_no_good_br!(sample_small_enough, sample);
                    return Ok(sample);
                }
                None if run == 1 && sample.q + 1.0 < sample.min_q => {
                    sample.set_q((sample.q + sample.min_q) / 2.0);
                }
                None => sample.set_q(sample.min_q),
            };
        } else {
            // Not Good Enough

            // is the encoding too big or using maximum bitrate?
            if !sample_small_enough || sample.q == sample.max_q {
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
                    sample.vmaf_lerp_q(*min_vmaf, None, Some(upper));
                }
                None if run == 1 && sample.q > sample.max_q + 1.0 => {
                    sample.set_q((sample.max_q + sample.q) / 2.0);
                }
                None => sample.set_q(sample.max_q),
            };
        }
        sample.print_attempt(&bar, *min_vmaf, *max_encoded_percent, *quiet, from_cache);
    }

    unreachable!();
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub enc: sample_encode::Output,
    val: u32,
    prev: (u32, f64),
    inc: u32,
    q: f64,
    min_q: f64,
    max_q: f64,
    transform: TransformValue,
}

impl Sample {
    pub fn br(&self) -> u32 {
        self.prev.0
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
        let mut percent = style!("{:.1}%", self.enc.encode_percent);
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

    fn new(
        enc: sample_encode::Output,
        min_br: u32,
        max_br: u32,
        br_increment: u32,
        transform: Transform,
    ) -> Self {
        let transform = TransformValue(transform);
        let min_q = transform.calc(f64::from(min_br));
        let max_q = transform.calc(f64::from(max_br));
        let q: f64 = (min_q + max_q) / 2.0;
        let val = Sample::num_from_q(q, br_increment, &transform);

        Sample {
            enc,
            val,
            prev: (val, q),
            inc: br_increment,
            q,
            min_q,
            max_q,
            transform,
        }
    }

    fn mod_inc(val: f64, inc: u32) -> u32 {
        (val as u32 / inc) * inc
    }

    fn num_from_q(q: f64, inc: u32, transform: &TransformValue) -> u32 {
        let val = transform.inverse(q) as u32;
        (val / inc) * inc
    }

    fn num_from_val(val: u32, inc: u32, transform: &TransformValue) -> f64 {
        let q = transform.calc(f64::from(val));
        (q / f64::from(inc)).round() * f64::from(inc)
    }

    fn from_q(&self) -> u32 {
        Sample::num_from_q(self.q, self.inc, &self.transform)
    }

    fn from_val(&self) -> f64 {
        Sample::num_from_val(self.val, self.inc, &self.transform)
    }

    fn q_from_val(&mut self) {
        self.q = self.from_val();
    }

    fn val_from_q(&mut self) {
        self.val = self.from_q();
    }

    fn set_q(&mut self, q: f64) {
        self.val_to_prev();
        self.q = q;
        self.val_from_q();
    }

    fn set_val(&mut self, val: u32) {
        self.val_to_prev();
        self.val = val;
        self.q_from_val();
    }

    fn val_to_prev(&mut self) {
        self.prev = (self.val, self.q);
    }

    /// Linear interpolation of new q based on
    ///
    /// y - y0   y1 - y0
    /// ------ = -------
    /// x - x0   x1 - x0
    ///
    /// Non-linear relationships are addressed through the transform field
    ///
    fn vmaf_lerp_q(&mut self, min_vmaf: f32, worse_q: Option<&Sample>, better_q: Option<&Sample>) {
        let (worse_q, worse_vmaf) = match worse_q {
            Some(worse) => (worse.q, worse.enc.vmaf),
            None => (self.q, self.enc.vmaf),
        };
        let (better_q, better_vmaf) = match better_q {
            Some(better) => (better.q, better.enc.vmaf),
            None => (self.q, self.enc.vmaf),
        };

        assert!(
            worse_vmaf <= min_vmaf && worse_vmaf < better_vmaf && worse_q < better_q,
            "invalid vmaf_lerp_br usage: ({min_vmaf}, {worse_q:?}, {better_q:?})"
        );

        let lerp = (worse_q * (better_vmaf - min_vmaf) as f64
            + better_q * (min_vmaf - worse_vmaf) as f64)
            / (better_vmaf - worse_vmaf) as f64;
        self.set_q(lerp.clamp(worse_q + 1.0, better_q - 1.0));
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
                let percent = style!("{:.1}%", enc.encode_percent).bold().green();
                let time = style(HumanDuration(enc.predicted_encode_time)).bold();
                let enc_description = match image {
                    true => "image",
                    false => "video stream",
                };
                println!(
                    "bitrate {br} VMAF {vmaf:.2} predicted {enc_description} size {size} ({percent}) taking {time}"
                );
            }
        }
    }
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

#[derive(Debug, Clone)]
enum Transform {
    Linear,
    Sqrt,
    Ln,
}
trait Transformation {
    fn calc(&self, val: f64) -> f64;

    fn inverse(&self, val: f64) -> f64;
}

#[derive(Debug, Clone)]
struct TransformValue(Transform);
impl Transformation for TransformValue {
    fn calc(&self, val: f64) -> f64 {
        match self.0 {
            Transform::Linear => f64::from(val) as _,
            Transform::Sqrt => f64::from(val).powi(2) as _,
            Transform::Ln => f64::from(val).exp() as _,
        }
    }

    fn inverse(&self, val: f64) -> f64 {
        match self.0 {
            Transform::Linear => f64::from(val) as _,
            Transform::Sqrt => f64::from(val).sqrt() as _,
            Transform::Ln => f64::from(val).ln() as _,
        }
    }
}

// mod test {
//     use super::*;

//     #[test]
//     fn q_br_lin_conversions() {
//         assert_eq!(q_from_br(12500, 10), 1250);
//         assert_eq!(q_from_br(5000, 100), 50);
//     }

//     // #[test]
//     // fn q_br_ln_conversions() {
//     //     assert_eq!(q_from_br_ln(12500, 10), 9.433483923290392);
//     //     assert_eq!(q_from_br_ln(5000, 10), 8.517193191416238);
//     // }

//     // #[test]
//     // fn q_br_sqrt_conversions() {
//     //     assert_eq!(q_from_br_sqrt(12500, 10), 111.80339887498948);
//     //     assert_eq!(q_from_br_sqrt(5000, 10), 70.71067811865476);
//     // }
// }
