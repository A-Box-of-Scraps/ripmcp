use crate::error::{Error, ErrorKind};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Timeouts {
    pub operation_seconds: NonZeroU64,
    pub login_seconds: NonZeroU64,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            operation_seconds: NonZeroU64::new(60).unwrap(),
            login_seconds: NonZeroU64::new(300).unwrap(),
        }
    }
}

pub struct Deadline {
    started: Instant,
    budget: Duration,
}

impl Deadline {
    pub fn new(budget: Duration) -> Self {
        Self {
            started: Instant::now(),
            budget,
        }
    }

    pub fn remaining(&self) -> Result<Duration, Error> {
        self.budget
            .checked_sub(self.started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::Timeout,
                    "operation timed out; completion may be unknown",
                )
            })
    }
}
