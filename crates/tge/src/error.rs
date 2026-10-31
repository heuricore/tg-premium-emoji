#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Usage(String),
    #[error("{0} validation issue(s) found")]
    Invalid(usize),
    #[error("{0}")]
    Runtime(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Usage(_) => 2,
            Error::Invalid(_) => 3,
            _ => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn exit_codes() {
        assert_eq!(Error::Usage("x".into()).exit_code(), 2);
        assert_eq!(Error::Invalid(4).exit_code(), 3);
        let io = std::io::Error::other("boom");
        assert_eq!(Error::Io(io).exit_code(), 1);
    }
}
