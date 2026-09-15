//! Helpers for the nested-`Option` shapes the wire-faithful contract types use
//! for effect's optional/nullable field flavours (see the crate docs).

/// Present value of an effect `optional(X)` field (`Option<Option<T>>`).
/// Absent and wire-`null` both decode to TS `undefined`, so only
/// `Some(Some(v))` carries a value.
pub fn defined2<T>(field: &Option<Option<T>>) -> Option<&T> {
    field.as_ref().and_then(|inner| inner.as_ref())
}

/// TS-visible state of an effect `optional(NullOr(X))` / `optionalKey(NullOr(X))`
/// field (`Option<Option<Option<T>>>`): `None` = TS `undefined` (skip),
/// `Some(None)` = TS `null` (meaningful), `Some(Some(&v))` = value.
pub fn defined3<T>(field: &Option<Option<Option<T>>>) -> Option<Option<&T>> {
    field
        .as_ref()
        .map(|middle| middle.as_ref().and_then(|inner| inner.as_ref()))
}

/// Wire form of TS `null` for a triple-`Option` field.
pub fn null3<T>() -> Option<Option<Option<T>>> {
    Some(None)
}

/// Wire form of a value for a triple-`Option` field.
pub fn value3<T>(value: T) -> Option<Option<Option<T>>> {
    Some(Some(Some(value)))
}
