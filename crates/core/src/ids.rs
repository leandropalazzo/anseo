//! Stable, sortable IDs for OpenGEO entities.
//!
//! Every entity uses a [ULID](https://github.com/ulid/spec)-backed newtype:
//! 128-bit, lexicographically sortable by creation time, URL-safe Crockford
//! base32 string form. The newtypes prevent passing the wrong ID type to a
//! function at the type level.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ulid::Ulid;

macro_rules! ulid_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Ulid);

        impl $name {
            /// Generate a fresh ULID at the current monotonic instant.
            pub fn new() -> Self {
                Self(Ulid::new())
            }

            pub fn from_ulid(u: Ulid) -> Self {
                Self(u)
            }

            pub fn into_ulid(self) -> Ulid {
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

        impl FromStr for $name {
            type Err = ulid::DecodeError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ulid::from_string(s).map(Self)
            }
        }

        // ULIDs cross wire boundaries as Crockford base32 strings, so the
        // OpenAPI / JSON-Schema view is `string` with the `ulid` format hint.
        impl schemars::JsonSchema for $name {
            fn schema_name() -> String {
                stringify!($name).to_owned()
            }
            fn json_schema(
                gen: &mut schemars::gen::SchemaGenerator,
            ) -> schemars::schema::Schema {
                let mut schema = <String as schemars::JsonSchema>::json_schema(gen);
                if let schemars::schema::Schema::Object(ref mut obj) = schema {
                    obj.format = Some("ulid".into());
                }
                schema
            }
        }

        impl utoipa::PartialSchema for $name {
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
                use utoipa::openapi::schema::{ObjectBuilder, SchemaType, Type};
                utoipa::openapi::RefOr::T(utoipa::openapi::schema::Schema::Object(
                    ObjectBuilder::new()
                        .schema_type(SchemaType::Type(Type::String))
                        .format(Some(utoipa::openapi::SchemaFormat::Custom("ulid".into())))
                        .build(),
                ))
            }
        }

        impl utoipa::ToSchema for $name {
            fn name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($name))
            }
        }
    };
}

ulid_newtype!(ProjectId, "Stable ID for a Project (FR-22).");
ulid_newtype!(PromptId, "Stable ID for a tracked Prompt (FR-1, FR-9).");
ulid_newtype!(
    PromptRunId,
    "Stable ID for a Prompt Run (FR-2). Also serves as the Postgres PK and idempotency key per ARCH C-6."
);
ulid_newtype!(MentionId, "Stable ID for an extracted Mention (FR-3).");
ulid_newtype!(CitationId, "Stable ID for an extracted Citation (FR-4).");
ulid_newtype!(
    RequestId,
    "Per-request correlation ID. Threaded through tracing spans and the `X-OpenGEO-Request-Id` response header (architecture L632)."
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn ulid_round_trip() {
        let project = ProjectId::new();
        let restored: ProjectId = project.to_string().parse().unwrap();
        assert_eq!(project, restored);

        let request = RequestId::new();
        let restored: RequestId = request.to_string().parse().unwrap();
        assert_eq!(request, restored);
    }

    #[test]
    fn ulids_sort_by_creation_time() {
        let id1 = PromptRunId::new();
        sleep(Duration::from_millis(2));
        let id2 = PromptRunId::new();
        sleep(Duration::from_millis(2));
        let id3 = PromptRunId::new();
        assert!(id1 < id2);
        assert!(id2 < id3);
    }

    #[test]
    fn serde_transparent() {
        let id = MentionId::new();
        let json = serde_json::to_string(&id).unwrap();
        // Transparent => quoted ULID string, no JSON object wrapping.
        assert!(json.starts_with('"') && json.ends_with('"'));
        assert!(!json.contains('{'));
        assert!(!json.contains(':'));
        let restored: MentionId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, restored);
    }
}
