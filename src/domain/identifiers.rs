//! Strongly typed external identifiers.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;
use uuid::{Uuid, Variant, Version};

const ENCODED_LEN: usize = 26;
const ALPHABET: &[u8; 32] = b"0123456789abcdefghijklmnopqrstuv";

/// Identifier parsing and validation failure.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdError {
    /// The public prefix does not identify the expected resource type.
    #[error("expected identifier prefix {expected}_")]
    WrongPrefix {
        /// Expected resource prefix.
        expected: &'static str,
    },
    /// The compact payload has the wrong size.
    #[error("identifier payload must contain exactly 26 base32hex characters")]
    InvalidLength,
    /// The payload is not canonical lowercase base32hex.
    #[error("identifier payload is not canonical lowercase base32hex")]
    NonCanonicalEncoding,
    /// The UUID does not use version 7 and the RFC 4122 variant.
    #[error("identifier payload is not an RFC 4122 UUID version 7")]
    NotUuidV7,
}

/// Behavior shared by every typed Proqi identifier.
trait TypedId:
    Copy + Eq + Ord + std::hash::Hash + fmt::Debug + fmt::Display + FromStr<Err = IdError>
{
    /// Stable public resource prefix.
    const PREFIX: &'static str;

    /// Validate and wrap one `UUIDv7`.
    ///
    /// # Errors
    ///
    /// Returns [`IdError::NotUuidV7`] for any other UUID version or variant.
    fn from_uuid(uuid: Uuid) -> Result<Self, IdError>;
}

macro_rules! define_id {
    ($name:ident, $prefix:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(Uuid);

        impl TypedId for $name {
            const PREFIX: &'static str = $prefix;

            fn from_uuid(uuid: Uuid) -> Result<Self, IdError> {
                validate_v7(uuid)?;
                Ok(Self(uuid))
            }
        }

        impl $name {
            /// Validate and wrap one `UUIDv7`.
            ///
            /// # Errors
            ///
            /// Returns an error for another UUID version or variant.
            pub fn from_uuid(uuid: Uuid) -> Result<Self, IdError> {
                <Self as TypedId>::from_uuid(uuid)
            }

            /// Return the complete UUID.
            #[must_use]
            pub const fn into_uuid(self) -> Uuid {
                self.0
            }

            /// Return the compact 16-byte SQLite representation.
            #[must_use]
            pub const fn database_bytes(self) -> [u8; 16] {
                *self.0.as_bytes()
            }

            /// Validate a compact SQLite representation.
            ///
            /// # Errors
            ///
            /// Returns an error when the bytes are not an RFC 4122 `UUIDv7`.
            pub fn from_database_bytes(bytes: [u8; 16]) -> Result<Self, IdError> {
                Self::from_uuid(Uuid::from_bytes(bytes))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(formatter, "{}_{}", Self::PREFIX, encode(self.0.as_bytes()))
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(self, formatter)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let payload = value
                    .strip_prefix(concat!($prefix, "_"))
                    .ok_or(IdError::WrongPrefix { expected: $prefix })?;
                let bytes = decode(payload)?;
                Self::from_database_bytes(bytes)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                value.parse().map_err(de::Error::custom)
            }
        }
    };
}

define_id!(SessionId, "ses", "Durable session identity.");
define_id!(ThoughtId, "tht", "Durable thought identity.");
define_id!(RevisionId, "rev", "Durable editor revision identity.");
define_id!(OperationId, "op", "Durable structural operation identity.");
define_id!(InstanceId, "ins", "Running Proqi process identity.");
define_id!(
    RequestId,
    "req",
    "Idempotent local-control request identity."
);
define_id!(
    SubmissionId,
    "sub",
    "Proqi-created submission receipt identity."
);

fn validate_v7(uuid: Uuid) -> Result<(), IdError> {
    if uuid.get_version() == Some(Version::SortRand) && uuid.get_variant() == Variant::RFC4122 {
        Ok(())
    } else {
        Err(IdError::NotUuidV7)
    }
}

fn encode(bytes: &[u8; 16]) -> String {
    let mut output = String::with_capacity(ENCODED_LEN);
    let mut buffer = 0_u16;
    let mut bits = 0_u8;
    for byte in bytes {
        buffer = (buffer << 8) | u16::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let index = usize::from((buffer >> bits) & 0x1f);
            output.push(char::from(ALPHABET[index]));
        }
        if bits > 0 {
            buffer &= (1_u16 << bits) - 1;
        } else {
            buffer = 0;
        }
    }
    if bits > 0 {
        let index = usize::from((buffer << (5 - bits)) & 0x1f);
        output.push(char::from(ALPHABET[index]));
    }
    debug_assert_eq!(output.len(), ENCODED_LEN);
    output
}

fn decode(payload: &str) -> Result<[u8; 16], IdError> {
    if payload.len() != ENCODED_LEN {
        return Err(IdError::InvalidLength);
    }
    let mut output = [0_u8; 16];
    let mut output_index = 0;
    let mut buffer = 0_u16;
    let mut bits = 0_u8;
    for character in payload.bytes() {
        let value = match character {
            b'0'..=b'9' => character - b'0',
            b'a'..=b'v' => character - b'a' + 10,
            _ => return Err(IdError::NonCanonicalEncoding),
        };
        buffer = (buffer << 5) | u16::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            if output_index >= output.len() {
                return Err(IdError::NonCanonicalEncoding);
            }
            output[output_index] =
                u8::try_from(buffer >> bits).map_err(|_| IdError::NonCanonicalEncoding)?;
            output_index += 1;
            if bits > 0 {
                buffer &= (1_u16 << bits) - 1;
            } else {
                buffer = 0;
            }
        }
    }
    let padding_mask = (1_u16 << bits) - 1;
    if output_index != output.len() || buffer & padding_mask != 0 {
        return Err(IdError::NonCanonicalEncoding);
    }
    Ok(output)
}
