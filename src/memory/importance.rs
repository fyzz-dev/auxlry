use chrono::{DateTime, Utc};

/// Compute earned importance score from access patterns and graph connectivity.
///
/// - Recency: exponential decay with 7-day half-life
/// - Access: ln(1 + access_count), floor 0.1
/// - Connectivity: ln(1 + edge_count), floor 0.1
/// - Result: recency * access * connectivity
pub fn compute_importance(
    access_count: u64,
    edge_count: u64,
    last_accessed_at: &str,
    now: &DateTime<Utc>,
) -> f64 {
    // Recency: exponential decay, 7-day half-life
    let recency = match DateTime::parse_from_rfc3339(last_accessed_at) {
        Ok(accessed) => {
            let hours = (*now - accessed.to_utc())
                .num_hours()
                .max(0) as f64;
            let half_life_hours = 7.0 * 24.0; // 168 hours
            (-hours * (2.0_f64.ln()) / half_life_hours).exp()
        }
        Err(_) => {
            // Try SQLite datetime format: "YYYY-MM-DD HH:MM:SS"
            match chrono::NaiveDateTime::parse_from_str(last_accessed_at, "%Y-%m-%d %H:%M:%S") {
                Ok(naive) => {
                    let accessed = naive.and_utc();
                    let hours = (*now - accessed).num_hours().max(0) as f64;
                    let half_life_hours = 7.0 * 24.0;
                    (-hours * (2.0_f64.ln()) / half_life_hours).exp()
                }
                Err(_) => 0.5, // fallback for unparseable timestamps
            }
        }
    };

    // Access factor
    let access = (1.0 + access_count as f64).ln().max(0.1);

    // Connectivity factor
    let connectivity = (1.0 + edge_count as f64).ln().max(0.1);

    recency * access * connectivity
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn recent_scores_high() {
        let n = now();
        let recent = n.to_rfc3339();
        let score = compute_importance(5, 3, &recent, &n);
        assert!(score > 0.5, "recent memory should score high: {score}");
    }

    #[test]
    fn old_memory_decays() {
        let n = now();
        let two_weeks_ago = (n - chrono::Duration::days(14)).to_rfc3339();
        let recent = n.to_rfc3339();

        let score_old = compute_importance(5, 3, &two_weeks_ago, &n);
        let score_new = compute_importance(5, 3, &recent, &n);

        assert!(
            score_old < score_new,
            "old memory should score lower: {score_old} vs {score_new}"
        );

        // 14 days = 2 half-lives, so recency should be ~0.25
        let recency_ratio = score_old / score_new;
        assert!(
            (recency_ratio - 0.25).abs() < 0.05,
            "14-day decay should be ~0.25: {recency_ratio}"
        );
    }

    #[test]
    fn zero_access_has_floor() {
        let n = now();
        let recent = n.to_rfc3339();
        let score = compute_importance(0, 0, &recent, &n);
        // access=0.1 (floor), connectivity=0.1 (floor), recency=1.0
        assert!(score > 0.0, "zero access should still have score: {score}");
        assert!(
            (score - 0.01).abs() < 0.001,
            "score should be ~0.01 (0.1 * 0.1): {score}"
        );
    }

    #[test]
    fn high_connectivity_boosts() {
        let n = now();
        let recent = n.to_rfc3339();
        let low = compute_importance(1, 0, &recent, &n);
        let high = compute_importance(1, 10, &recent, &n);
        assert!(
            high > low,
            "high connectivity should boost: {high} vs {low}"
        );
    }

    #[test]
    fn sqlite_datetime_format() {
        let n = now();
        let sqlite_ts = n.format("%Y-%m-%d %H:%M:%S").to_string();
        let score = compute_importance(5, 3, &sqlite_ts, &n);
        assert!(score > 0.5, "sqlite format should parse: {score}");
    }
}
