use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
    FireBam,
    Other(u8),
}

impl Serialize for ClientType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            ClientType::SolanaLabs => "SolanaLabs",
            ClientType::JitoLabs => "JitoLabs",
            ClientType::Firedancer => "Firedancer",
            ClientType::Agave => "Agave",
            ClientType::Bam => "BAM",
            ClientType::FireBam => "FireBAM",
            ClientType::Other(_) => "Other",
        })
    }
}

impl<'de> Deserialize<'de> for ClientType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "SolanaLabs" => ClientType::SolanaLabs,
            "JitoLabs" => ClientType::JitoLabs,
            "Firedancer" => ClientType::Firedancer,
            "Agave" => ClientType::Agave,
            "BAM" => ClientType::Bam,
            "FireBAM" => ClientType::FireBam,
            _ => ClientType::Other(u8::MAX),
        })
    }
}

impl Default for ClientType {
    fn default() -> Self {
        ClientType::Other(u8::MAX)
    }
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
    /// - 6: BAM
    /// - 12: FireBAM
    /// - Other values: Stored as `Other(value)`
    ///
    /// https://github.com/anza-xyz/agave/blob/master/version/src/lib.rs#L19
    ///
    /// # Examples
    ///
    /// ```
    /// let client = ClientType::from_u8(1);
    /// assert_eq(client, ClientType::JitoLabs);
    ///
    /// let client = ClientType::from_u8(99);
    /// assert_eq(client, ClientType::Other(99));
    /// ```
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => ClientType::SolanaLabs,
            1 => ClientType::JitoLabs,
            2 => ClientType::Firedancer,
            3 => ClientType::Agave,
            6 => ClientType::Bam,
            12 => ClientType::FireBam,
            other => ClientType::Other(other),
        }
    }

    /// Returns true when the client can run BAM transaction forwarding.
    pub fn is_bam_capable(&self) -> bool {
        matches!(
            self,
            ClientType::JitoLabs | ClientType::Bam | ClientType::FireBam
        )
    }

    /// Returns the string representation of the client type.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(ClientType::JitoLabs.as_str(), "Jito Labs");
    /// assert_eq!(ClientType::Other(99).as_str(), "Other");
    /// ```
    fn as_str(&self) -> &'static str {
        match self {
            ClientType::SolanaLabs => "Solana Labs",
            ClientType::JitoLabs => "Jito Labs",
            ClientType::Firedancer => "Firedancer",
            ClientType::Agave => "Agave",
            ClientType::Bam => "BAM",
            ClientType::FireBam => "FireBAM",
            ClientType::Other(_) => "Other",
        }
    }
}

impl std::fmt::Display for ClientType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
