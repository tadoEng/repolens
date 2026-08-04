//! Opaque, time-ordered domain identifiers.
//!
//! UUID version 7: sortable by creation time, which gives PostgreSQL index
//! locality on insert, while retaining 74 random bits — enough that a public
//! report URL is unguessable even though anonymous readers need no credential
//! to open it.
//! The embedded timestamp is not a leak here: every report already displays
//! when it was analyzed.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$outer:meta])* $name:ident) => {
        $(#[$outer])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a fresh time-ordered identifier.
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Adopts an identifier that already exists, typically read back
            /// from storage or parsed from a request path.
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            /// Unwraps to the underlying UUID.
            pub const fn into_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

id_type! {
    /// Identifies one analysis run, and therefore one public report URL.
    AnalysisId
}

id_type! {
    /// Identifies the resolved repository state an analysis was run against.
    SnapshotId
}

id_type! {
    /// Identifies a single finding within a report.
    FindingId
}

#[cfg(test)]
mod tests {
    use super::AnalysisId;

    #[test]
    fn generated_ids_are_version_7() {
        assert_eq!(AnalysisId::new().into_uuid().get_version_num(), 7);
    }

    #[test]
    fn generated_ids_are_distinct() {
        assert_ne!(AnalysisId::new(), AnalysisId::new());
    }

    #[test]
    fn display_round_trips_through_uuid() {
        let id = AnalysisId::new();
        assert_eq!(id.to_string(), id.into_uuid().to_string());
    }
}
