use std::time::{Duration, Instant};

pub const WRITE_COOLDOWN: Duration = Duration::from_secs(600);
pub const MAX_ENGAGEMENTS_PER_NIGHT: u32 = 2;

pub fn can_write(grid_on: Option<bool>, dry_run: bool, last_write_at: Option<Instant>, now: Instant) -> bool {
    if dry_run {
        return false;
    }
    if grid_on == Some(false) {
        return false;
    }
    !last_write_at.is_some_and(|written| now.saturating_duration_since(written) < WRITE_COOLDOWN)
}

pub fn below_floor(soc: Option<f64>, min_soc_percent: f64) -> bool {
    soc.is_some_and(|soc| soc < min_soc_percent)
}

pub fn user_took_over(current_mode: Option<u32>, last_written_mode: Option<u32>, dry_run: bool) -> bool {
    if dry_run {
        return false;
    }
    match last_written_mode {
        Some(written) => current_mode.is_some_and(|current| current != written),
        None => false,
    }
}

pub fn engagement_cap_reached(engagements: u32) -> bool {
    engagements >= MAX_ENGAGEMENTS_PER_NIGHT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_write_blocks_dry_run() {
        assert!(!can_write(Some(true), true, None, Instant::now()));
    }

    #[test]
    fn can_write_blocks_grid_off() {
        assert!(!can_write(Some(false), false, None, Instant::now()));
    }

    #[test]
    fn can_write_respects_cooldown() {
        let now = Instant::now();
        assert!(!can_write(Some(true), false, Some(now), now));
        let past = now - Duration::from_secs(601);
        assert!(can_write(Some(true), false, Some(past), now));
    }

    #[test]
    fn can_write_allows_first_write() {
        assert!(can_write(Some(true), false, None, Instant::now()));
    }

    #[test]
    fn below_floor_needs_soc_data() {
        assert!(below_floor(Some(19.0), 20.0));
        assert!(!below_floor(Some(20.0), 20.0));
        assert!(!below_floor(None, 20.0));
    }

    #[test]
    fn user_takeover_ignored_in_dry_run() {
        assert!(!user_took_over(Some(0), Some(1), true));
    }

    #[test]
    fn user_takeover_detects_mode_change() {
        assert!(user_took_over(Some(0), Some(1), false));
        assert!(!user_took_over(Some(1), Some(1), false));
        assert!(!user_took_over(Some(0), None, false));
    }

    #[test]
    fn engagement_cap_at_two_per_night() {
        assert!(!engagement_cap_reached(1));
        assert!(engagement_cap_reached(2));
        assert!(engagement_cap_reached(3));
    }
}
