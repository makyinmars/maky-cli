use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "maky",
    bin_name = "maky",
    version,
    about = "A lightweight agent CLI built while learning Rust",
    long_about = None
)]
pub struct Cli {
    /// Resume a specific persisted session id.
    #[arg(long, value_name = "SESSION_ID", conflicts_with = "new_session")]
    pub resume: Option<String>,

    /// Start a fresh session and skip automatic latest-session restore.
    #[arg(long = "new", conflicts_with = "resume")]
    pub new_session: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_resume_argument() {
        let cli = Cli::parse_from(["maky", "--resume", "session-123"]);

        assert_eq!(cli.resume.as_deref(), Some("session-123"));
        assert!(!cli.new_session);
    }

    #[test]
    fn parses_new_argument() {
        let cli = Cli::parse_from(["maky", "--new"]);

        assert!(cli.new_session);
        assert!(cli.resume.is_none());
    }
}
