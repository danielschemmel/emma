pub mod hl {
    use std::time::{Duration, Instant};

  pub struct Timer {
    start: Instant,
    elapsed: Duration,
  }

  impl Timer {
    pub fn new() -> Self {
      Self {
        start: Instant::now(),
        elapsed: Duration::ZERO,
      }
    }

    pub fn start(&mut self) {
      self.start = Instant::now();
    }

    pub fn stop(&mut self) {
      self.elapsed += Instant::now() - self.start;
    }

    pub fn reset(&mut self) {
      self.elapsed = Duration::ZERO;
    }

    pub fn elapsed_seconds(&self) -> f64 {
      self.elapsed.as_secs_f64()
    }
  }
}

pub use hl::*;