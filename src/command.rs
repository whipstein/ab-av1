pub mod args;
pub mod auto_encode;
pub mod bitrate_search;
// pub mod crf_search;
pub mod cq_search;
pub mod encode;
pub mod encoders;
pub mod print_completions;
pub mod probe;
pub mod sample_encode;
pub mod vmaf;

pub use auto_encode::auto_encode;
pub use cq_search::cq_search;
// pub use crf_search::crf_search;
pub use bitrate_search::bitrate_search;
pub use encode::encode;
pub use print_completions::print_completions;
pub use probe::probe;
pub use sample_encode::sample_encode;
pub use vmaf::vmaf;

const PROGRESS_CHARS: &str = "##-";

/// Helper trait for durations under 584942 years or so.
trait SmallDuration {
    /// Returns the total number of whole microseconds.
    fn as_micros_u64(&self) -> u64;
}

impl SmallDuration for std::time::Duration {
    fn as_micros_u64(&self) -> u64 {
        self.as_micros().try_into().unwrap_or(u64::MAX)
    }
}
