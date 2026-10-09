#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    InvalidEndpoint,
    Transport,
    Timeout,
    Http(u16),
    InvalidResponse,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidEndpoint => f.write_str(
                "Use a loopback HTTP /records URL without credentials, query or fragment.",
            ),
            Self::Transport => f.write_str("Could not reach the catalog."),
            Self::Timeout => f.write_str("Catalog request timed out."),
            Self::Http(status) => write!(f, "Catalog returned HTTP {status}."),
            Self::InvalidResponse => f.write_str("Catalog returned invalid record data."),
        }
    }
}
impl std::error::Error for Error {}
