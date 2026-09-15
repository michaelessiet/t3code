//! Hand-written support types referenced by the generated contract code.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// One RPC method of the sidecar's WebSocket surface. `TAG` is the wire tag
/// sent in Request envelopes; for `STREAM` methods `Success` is the per-event
/// stream element, not a final response.
pub trait RpcMethod {
    const TAG: &'static str;
    const STREAM: bool;
    type Payload: Serialize;
    type Success: DeserializeOwned + std::fmt::Debug;
    type Error: DeserializeOwned + std::fmt::Debug;
}

/// Wire encoding of effect's `Schema.Option(T)` under the rpc JSON codec:
/// `{"_tag": "Some", "value": T}` | `{"_tag": "None"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum EffectOption<T> {
    Some { value: T },
    None,
}

impl<T> EffectOption<T> {
    pub fn into_option(self) -> Option<T> {
        match self {
            EffectOption::Some { value } => Some(value),
            EffectOption::None => None,
        }
    }
}

impl<T> From<Option<T>> for EffectOption<T> {
    fn from(value: Option<T>) -> Self {
        match value {
            Some(value) => EffectOption::Some { value },
            None => EffectOption::None,
        }
    }
}

/// Wire encoding of effect's `Schema.DurationFromMillis`: a number of
/// milliseconds, or one of the string sentinels `"Infinity"`, `"-Infinity"`,
/// `"NaN"` (effect Durations can be infinite).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DurationMillis {
    // serde_json::Number (not f64) so integer millis round-trip byte-exactly.
    Millis(serde_json::Number),
    Sentinel(String),
}

impl DurationMillis {
    pub fn as_millis_f64(&self) -> Option<f64> {
        match self {
            DurationMillis::Millis(n) => n.as_f64(),
            DurationMillis::Sentinel(_) => None,
        }
    }
}

/// Serde adapter distinguishing an absent key from an explicit `null` for
/// effect's `optionalKey(NullOr(T))` / `optional(NullOr(T))` fields. Use with
/// `#[serde(default, skip_serializing_if = "Option::is_none", with = "double_option")]`
/// on an `Option<Option<T>>` field: absent = `None`, `null` = `Some(None)`.
pub mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        Option::<T>::deserialize(deserializer).map(Some)
    }

    pub fn serialize<S, T>(value: &Option<Option<T>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        match value {
            // skip_serializing_if handles the outer None; this arm is
            // unreachable in practice but must serialize something total.
            None => serializer.serialize_none(),
            Some(inner) => inner.serialize(serializer),
        }
    }
}

#[cfg(test)]
pub(crate) fn assert_fixture_roundtrip<T>(fixture_json: &str)
where
    T: Serialize + DeserializeOwned,
{
    #[derive(Deserialize)]
    struct Fixture {
        schema: String,
        encoded: serde_json::Value,
    }
    let fixture: Fixture = serde_json::from_str(fixture_json).expect("fixture parses");
    let decoded: T = serde_json::from_value(fixture.encoded.clone())
        .unwrap_or_else(|e| panic!("{}: decode failed: {e}", fixture.schema));
    let re_encoded = serde_json::to_value(&decoded)
        .unwrap_or_else(|e| panic!("{}: encode failed: {e}", fixture.schema));
    assert_eq!(
        re_encoded, fixture.encoded,
        "{}: decode → encode is not a fixed point",
        fixture.schema
    );
}
