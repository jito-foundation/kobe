/// Represents the different validator client types
///
/// This enum maps the u8 values stored in the ValidatorHistory program to human-readable client
/// type names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientType {
    SolanaLabs,
    JitoLabs,
    Firedancer,
    Agave,
    Bam,
    Other(u8), // Store the unknown value for debugging
}

impl ClientType {
    /// Initialize [`ClientType`] from a u8 value
    ///
    /// # Overview
    ///
    /// The ValidatorHistory program stores client types as u8 values with the following mapping:
    /// - 0: Solana Labs
    /// - 1: Jito Labs
    /// - 2: Firedancer
    /// - 3: Agave
    /// - Other values: Stored as `Other(value)`
    ///
    /// https://github.com/anza-xyz/agave/blob/master/version/src/lib.rs#L19
    ///
    /// # Examples
    ///
    /// ```
    /// use kobe_core::client_type::ClientType;
    ///
    /// let client = ClientType::from_u8(1);
    /// assert_eq!(client, ClientType::JitoLabs);
    ///
    /// let client = ClientType::from_u8(99);
    /// assert_eq!(client, ClientType::Other(99));
    /// ```
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => ClientType::SolanaLabs,
            1 => ClientType::JitoLabs,
            2 => ClientType::Firedancer,
            3 => ClientType::Agave,
            6 => ClientType::Bam,
            other => ClientType::Other(other),
        }
    }

    /// Returns the string representation of the client type.
    ///
    /// # Examples
    ///
    /// ```
    /// use kobe_core::client_type::ClientType;
    ///
    /// assert_eq!(ClientType::JitoLabs.as_str(), "Jito Labs");
    /// assert_eq!(ClientType::Other(99).as_str(), "Other");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            ClientType::SolanaLabs => "Solana Labs",
            ClientType::JitoLabs => "Jito Labs",
            ClientType::Firedancer => "Firedancer",
            ClientType::Agave => "Agave",
            ClientType::Bam => "BAM",
            ClientType::Other(_) => "Other",
        }
    }
}

impl std::fmt::Display for ClientType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
