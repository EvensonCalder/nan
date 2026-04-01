use crate::error::NanError;

pub const INITIAL_STABILITY_DAYS: f64 = 0.018;
pub const MEMORY_BETA: f64 = 0.25;
pub const MEMORY_A: f64 = 0.6;
pub const MEMORY_B: f64 = 0.08;
const SECONDS_PER_DAY: f64 = 86_400.0;
const MIN_DELTA_DAYS: f64 = 1.0 / SECONDS_PER_DAY;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReviewState {
    pub s_last_days: f64,
    pub t_last_unix_secs: i64,
}

impl ReviewState {
    pub fn new(t_last_unix_secs: i64) -> Self {
        Self {
            s_last_days: INITIAL_STABILITY_DAYS,
            t_last_unix_secs,
        }
    }

    pub fn validate(self) -> Result<(), NanError> {
        if !self.s_last_days.is_finite() || self.s_last_days <= 0.0 {
            return Err(NanError::InvalidData(
                "review stability must be a finite positive number".to_string(),
            ));
        }

        Ok(())
    }
}

pub fn seconds_to_days(seconds: i64) -> f64 {
    (seconds as f64 / SECONDS_PER_DAY).max(MIN_DELTA_DAYS)
}

pub fn elapsed_days(last_unix_secs: i64, now_unix_secs: i64) -> f64 {
    seconds_to_days((now_unix_secs - last_unix_secs).max(1))
}

pub fn review_memory_score(state: ReviewState, now_unix_secs: i64) -> Result<f64, NanError> {
    state.validate()?;
    let delta_days = elapsed_days(state.t_last_unix_secs, now_unix_secs);
    let ratio = (delta_days / state.s_last_days).max(0.0);
    Ok((-(ratio.powf(MEMORY_BETA))).exp())
}

pub fn apply_review(state: ReviewState, now_unix_secs: i64) -> Result<ReviewState, NanError> {
    state.validate()?;

    if now_unix_secs < state.t_last_unix_secs {
        return Err(NanError::InvalidData(
            "review time cannot be earlier than the last review time".to_string(),
        ));
    }

    let delta_days = elapsed_days(state.t_last_unix_secs, now_unix_secs);
    let updated_stability = state.s_last_days
        * (1.0 + MEMORY_B + MEMORY_A * (1.0 + delta_days / state.s_last_days).ln());

    if !updated_stability.is_finite() || updated_stability <= 0.0 {
        return Err(NanError::InvalidData(
            "review update produced an invalid stability".to_string(),
        ));
    }

    Ok(ReviewState {
        s_last_days: updated_stability,
        t_last_unix_secs: now_unix_secs,
    })
}

pub fn review_priority(score: f64) -> f64 {
    1.0 - score.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{ReviewState, apply_review, review_memory_score};

    #[test]
    fn review_increases_stability() {
        let original = ReviewState::new(0);
        let updated = apply_review(original, 86_400).expect("review should update");
        assert!(updated.s_last_days > original.s_last_days);
        assert_eq!(updated.t_last_unix_secs, 86_400);
    }

    #[test]
    fn memory_score_decays_over_time() {
        let state = ReviewState::new(0);
        let near = review_memory_score(state, 3_600).expect("score should calculate");
        let far = review_memory_score(state, 86_400 * 5).expect("score should calculate");
        assert!(near > far);
        assert!((0.0..=1.0).contains(&near));
        assert!((0.0..=1.0).contains(&far));
    }
}
