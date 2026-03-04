use serde::{Deserialize, Serialize};

/// Cognitive memory types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,
    Decision,
    Inference,
    Preference,
    Observation,
    Event,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fact => "fact",
            Self::Decision => "decision",
            Self::Inference => "inference",
            Self::Preference => "preference",
            Self::Observation => "observation",
            Self::Event => "event",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "fact" => Some(Self::Fact),
            "decision" => Some(Self::Decision),
            "inference" => Some(Self::Inference),
            "preference" => Some(Self::Preference),
            "observation" => Some(Self::Observation),
            "event" => Some(Self::Event),
            _ => None,
        }
    }
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Classify memory content using keyword/pattern heuristics.
/// Falls back to `Observation` when no pattern matches.
pub fn classify_heuristic(content: &str) -> MemoryType {
    let lower = content.to_lowercase();

    // Decision patterns
    if lower.contains("decided to")
        || lower.contains("decision:")
        || lower.contains("we chose")
        || lower.contains("i chose")
        || lower.contains("chose to")
        || lower.contains("decided")
        || lower.contains("agreed to")
        || lower.contains("will use")
        || lower.contains("going with")
        || lower.contains("we'll go with")
        || lower.contains("settled on")
        || lower.contains("switching to")
        || lower.contains("migrating to")
        || lower.contains("adopting")
    {
        return MemoryType::Decision;
    }

    // Preference patterns
    if lower.contains("prefer")
        || lower.contains("preference:")
        || lower.contains("i like")
        || lower.contains("i want")
        || lower.contains("favorite")
        || lower.contains("rather than")
        || lower.contains("instead of")
        || lower.contains("i'd rather")
        || lower.contains("i don't like")
        || lower.contains("please always")
        || lower.contains("please never")
        || lower.contains("would rather")
    {
        return MemoryType::Preference;
    }

    // Inference patterns
    if lower.contains("therefore")
        || lower.contains("implies")
        || lower.contains("suggests that")
        || lower.contains("concluded")
        || lower.contains("it follows")
        || lower.contains("based on this")
        || lower.contains("likely because")
        || lower.contains("probably")
        || lower.contains("seems like")
        || lower.contains("appears to be")
        || lower.contains("might be")
        || lower.contains("could be")
        || lower.contains("looks like")
    {
        return MemoryType::Inference;
    }

    // Event patterns (temporal markers)
    if lower.contains("happened")
        || lower.contains("occurred")
        || lower.contains("yesterday")
        || lower.contains("today")
        || lower.contains("last week")
        || lower.contains("deployed")
        || lower.contains("released")
        || lower.contains("incident")
        || lower.contains("outage")
        || lower.contains("updated")
        || lower.contains("installed")
        || lower.contains("migrated")
        || lower.contains("crashed")
        || lower.contains("fixed")
        || lower.contains("merged")
        || lower.contains("pushed")
        || lower.contains("shipped")
    {
        return MemoryType::Event;
    }

    // Fact patterns
    if lower.contains("is a")
        || lower.contains("are a")
        || lower.contains("defined as")
        || lower.contains("means that")
        || lower.contains("equals")
        || lower.contains("fact:")
        || lower.contains("specification:")
        || lower.contains("the api")
        || lower.contains("version is")
        || lower.contains("runs on")
        || lower.contains("uses")
        || lower.contains("is built with")
        || lower.contains("is located at")
        || lower.contains("the url")
        || lower.contains("the port")
    {
        return MemoryType::Fact;
    }

    MemoryType::Observation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_decision() {
        assert_eq!(
            classify_heuristic("We decided to use PostgreSQL"),
            MemoryType::Decision
        );
        assert_eq!(
            classify_heuristic("Going with React for the frontend"),
            MemoryType::Decision
        );
    }

    #[test]
    fn classify_preference() {
        assert_eq!(
            classify_heuristic("I prefer tabs over spaces"),
            MemoryType::Preference
        );
        assert_eq!(
            classify_heuristic("I want dark mode by default"),
            MemoryType::Preference
        );
    }

    #[test]
    fn classify_inference() {
        assert_eq!(
            classify_heuristic("Therefore the service must be stateless"),
            MemoryType::Inference
        );
        assert_eq!(
            classify_heuristic("This suggests that latency is the bottleneck"),
            MemoryType::Inference
        );
    }

    #[test]
    fn classify_event() {
        assert_eq!(
            classify_heuristic("Deployed v2.3 to production yesterday"),
            MemoryType::Event
        );
        assert_eq!(
            classify_heuristic("A database outage occurred"),
            MemoryType::Event
        );
    }

    #[test]
    fn classify_fact() {
        assert_eq!(
            classify_heuristic("Rust is a systems programming language"),
            MemoryType::Fact
        );
        assert_eq!(
            classify_heuristic("The API version is 3.1"),
            MemoryType::Fact
        );
    }

    #[test]
    fn classify_decision_new_patterns() {
        assert_eq!(
            classify_heuristic("We chose to adopt microservices"),
            MemoryType::Decision
        );
        assert_eq!(
            classify_heuristic("Switching to a new database engine"),
            MemoryType::Decision
        );
        assert_eq!(
            classify_heuristic("Adopting Kubernetes for orchestration"),
            MemoryType::Decision
        );
    }

    #[test]
    fn classify_preference_new_patterns() {
        assert_eq!(
            classify_heuristic("I'd rather use vim than emacs"),
            MemoryType::Preference
        );
        assert_eq!(
            classify_heuristic("Please never auto-format my code"),
            MemoryType::Preference
        );
        assert_eq!(
            classify_heuristic("I don't like verbose logging"),
            MemoryType::Preference
        );
    }

    #[test]
    fn classify_inference_new_patterns() {
        assert_eq!(
            classify_heuristic("It seems like the cache is stale"),
            MemoryType::Inference
        );
        assert_eq!(
            classify_heuristic("This could be a race condition"),
            MemoryType::Inference
        );
        assert_eq!(
            classify_heuristic("Looks like the timeout is too low"),
            MemoryType::Inference
        );
    }

    #[test]
    fn classify_event_new_patterns() {
        assert_eq!(
            classify_heuristic("The server crashed at 3am"),
            MemoryType::Event
        );
        assert_eq!(
            classify_heuristic("We merged the auth refactor PR"),
            MemoryType::Event
        );
        assert_eq!(
            classify_heuristic("Shipped the new landing page"),
            MemoryType::Event
        );
    }

    #[test]
    fn classify_fact_new_patterns() {
        assert_eq!(
            classify_heuristic("The service runs on port 8080"),
            MemoryType::Fact
        );
        assert_eq!(
            classify_heuristic("The project is built with Rust"),
            MemoryType::Fact
        );
        assert_eq!(
            classify_heuristic("The URL for the dashboard is /admin"),
            MemoryType::Fact
        );
    }

    #[test]
    fn classify_observation_fallback() {
        assert_eq!(
            classify_heuristic("The sky looks nice"),
            MemoryType::Observation
        );
        assert_eq!(
            classify_heuristic("Some random text here"),
            MemoryType::Observation
        );
    }

    #[test]
    fn serde_roundtrip() {
        let mt = MemoryType::Decision;
        let json = serde_json::to_string(&mt).unwrap();
        assert_eq!(json, "\"decision\"");
        let back: MemoryType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, MemoryType::Decision);
    }
}
