//! Stage B of the Vitre contracts pipeline: reads
//! `crates/vitre-contracts/contracts.gen.json` (produced by
//! `node scripts/vitre/export-contracts.ts`) and emits
//! `crates/vitre-contracts/src/generated.rs` — serde types for every `$defs`
//! entry plus a typed RPC method table and fixture round-trip tests.
//!
//! Run: `cargo run -p vitre-contracts-gen && cargo fmt -p vitre-contracts`

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::PathBuf;

fn main() -> Result<()> {
    let contracts_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("vitre-contracts");
    let doc: Value = serde_json::from_str(
        &std::fs::read_to_string(contracts_dir.join("contracts.gen.json"))
            .context("read contracts.gen.json")?,
    )?;
    let defs = doc["$defs"]
        .as_object()
        .context("$defs object")?
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<BTreeMap<String, Value>>();
    let methods = doc["methods"].as_array().context("methods array")?.clone();

    let mut generator = Generator::new(defs);
    generator.compute_reachability();
    let mut out = String::new();
    out.push_str(HEADER);
    for name in generator.defs.keys().cloned().collect::<Vec<_>>() {
        let code = generator.emit_def(&name)?;
        out.push_str(&code);
    }
    out.push_str(&generator.emit_methods(&methods)?);
    out.push_str(&generator.emit_fixture_tests(&contracts_dir.join("fixtures"))?);

    let out_path = contracts_dir.join("src/generated.rs");
    std::fs::write(&out_path, out)?;
    println!(
        "wrote {} ({} defs, {} methods)",
        out_path.display(),
        generator.defs.len(),
        methods.len()
    );
    Ok(())
}

const HEADER: &str = "\
// GENERATED FILE — DO NOT EDIT.
// Source: contracts.gen.json (exported from packages/contracts by
// scripts/vitre/export-contracts.ts). Regenerate with:
//   node scripts/vitre/export-contracts.ts
//   node scripts/vitre/export-fixtures.ts
//   cargo run -p vitre-contracts-gen && cargo fmt -p vitre-contracts
#![allow(clippy::large_enum_variant)]

use crate::support::{DurationMillis, EffectOption};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the contracts.gen.json this module was generated from.
pub const CONTRACTS_VERSION: u32 = 1;

";

struct Generator {
    defs: BTreeMap<String, Value>,
    /// def name -> rust ident
    idents: HashMap<String, String>,
    /// def name -> set of def names reachable through direct (non-heap) positions
    reach: HashMap<String, HashSet<String>>,
}

impl Generator {
    fn new(defs: BTreeMap<String, Value>) -> Self {
        let mut idents = HashMap::new();
        let mut used = HashSet::new();
        for name in defs.keys() {
            let mut ident = pascal(name);
            while used.contains(&ident) {
                ident.push('X');
            }
            used.insert(ident.clone());
            idents.insert(name.clone(), ident);
        }
        Generator {
            defs,
            idents,
            reach: HashMap::new(),
        }
    }

    fn ident(&self, def_name: &str) -> String {
        self.idents
            .get(def_name)
            .cloned()
            .unwrap_or_else(|| pascal(def_name))
    }

    // ---- recursion analysis -------------------------------------------------

    fn compute_reachability(&mut self) {
        let mut direct: HashMap<String, HashSet<String>> = HashMap::new();
        for (name, schema) in &self.defs {
            let mut edges = HashSet::new();
            collect_direct_refs(schema, &mut edges);
            direct.insert(name.clone(), edges);
        }
        for name in self.defs.keys() {
            let mut seen = HashSet::new();
            let mut stack = vec![name.clone()];
            while let Some(cur) = stack.pop() {
                if let Some(edges) = direct.get(&cur) {
                    for e in edges {
                        if seen.insert(e.clone()) {
                            stack.push(e.clone());
                        }
                    }
                }
            }
            self.reach.insert(name.clone(), seen);
        }
    }

    /// True when embedding `target` directly inside `owner` would recurse.
    fn needs_box(&self, owner: &str, target: &str) -> bool {
        owner == target || self.reach.get(target).is_some_and(|r| r.contains(owner))
    }

    // ---- def emission -------------------------------------------------------

    fn emit_def(&self, name: &str) -> Result<String> {
        let schema = self.defs.get(name).unwrap().clone();
        let ident = self.ident(name);
        let mut ctx = EmitCtx {
            owner: name.to_string(),
            hoists: String::new(),
            hoist_names: HashSet::new(),
        };
        let mut out = String::new();

        if let Some(obj) = schema.as_object() {
            // pure alias
            if obj.len() == 1 && obj.contains_key("$ref") {
                let target = ref_name(obj["$ref"].as_str().unwrap())?;
                writeln!(out, "pub type {ident} = {};\n", self.ident(&target))?;
                return Ok(out);
            }
        }

        match self.classify(&schema) {
            Shape::EmptyPayload => {
                writeln!(
                    out,
                    "/// Void payload — send `{{}}` on the wire.\npub type {ident} = serde_json::Value;\n"
                )?;
            }
            Shape::Null => {
                writeln!(out, "pub type {ident} = ();\n")?;
            }
            Shape::Never => {
                writeln!(
                    out,
                    "/// `Schema.Never` — no value inhabits this type on the wire.\npub type {ident} = serde_json::Value;\n"
                )?;
            }
            Shape::Prim(prim) => {
                let extra = match prim {
                    "String" => ", Eq, Hash, PartialOrd, Ord",
                    "i64" => ", Eq, Hash, PartialOrd, Ord, Copy",
                    "bool" => ", Eq, Hash, Copy",
                    _ => ", Copy",
                };
                writeln!(
                    out,
                    "#[derive(Debug, Clone, PartialEq{extra}, Serialize, Deserialize)]\n#[serde(transparent)]\npub struct {ident}(pub {prim});\n"
                )?;
            }
            Shape::StringEnum(values) => {
                out.push_str(&self.emit_string_enum(&ident, &values)?);
            }
            Shape::NumberLiteral(prim) => {
                writeln!(out, "pub type {ident} = {prim};\n")?;
            }
            Shape::Nullable(inner) => {
                let t = self.rust_type(&inner, &mut ctx, true)?;
                writeln!(out, "pub type {ident} = Option<{t}>;\n")?;
            }
            Shape::EffectOption(inner) => {
                let t = self.rust_type(&inner, &mut ctx, true)?;
                writeln!(out, "pub type {ident} = EffectOption<{t}>;\n")?;
            }
            Shape::Duration => {
                writeln!(out, "pub type {ident} = DurationMillis;\n")?;
            }
            Shape::Array(items) => {
                let t = self.rust_type(&items, &mut ctx, false)?;
                writeln!(out, "pub type {ident} = Vec<{t}>;\n")?;
            }
            Shape::Record(values) => {
                let t = self.rust_type(&values, &mut ctx, false)?;
                writeln!(out, "pub type {ident} = BTreeMap<String, {t}>;\n")?;
            }
            Shape::BareObject => {
                writeln!(
                    out,
                    "pub type {ident} = serde_json::Map<String, serde_json::Value>;\n"
                )?;
            }
            Shape::Struct => {
                out.push_str(&self.emit_struct(&ident, &schema, &mut ctx)?);
            }
            Shape::TaggedUnion(disc, members) => {
                out.push_str(&self.emit_tagged_union(&ident, &disc, &members, &mut ctx)?);
            }
            Shape::StringyUnion(values) => {
                out.push_str(&self.emit_string_enum(&ident, &values)?);
            }
            Shape::UntaggedUnion(members) => {
                out.push_str(&self.emit_untagged_union(&ident, &members, &mut ctx)?);
            }
            Shape::Opaque => {
                writeln!(out, "pub type {ident} = serde_json::Value;\n")?;
            }
        }

        out.push_str(&ctx.hoists);
        Ok(out)
    }

    fn classify(&self, schema: &Value) -> Shape {
        let Some(obj) = schema.as_object() else {
            return Shape::Opaque;
        };
        if obj.contains_key("not") {
            return Shape::Never;
        }
        if let Some(any_of) = obj.get("anyOf").and_then(Value::as_array) {
            if is_empty_payload(any_of) {
                return Shape::EmptyPayload;
            }
            if let Some(inner) = effect_option_inner(any_of) {
                return Shape::EffectOption(inner);
            }
            if is_duration(any_of) {
                return Shape::Duration;
            }
            if any_of.len() == 2
                && let Some(inner) = nullable_inner(any_of)
            {
                return Shape::Nullable(inner);
            }
            if let Some((disc, members)) = self.detect_tagged(any_of) {
                return Shape::TaggedUnion(disc, members);
            }
            if let Some(values) = self.all_stringy(any_of) {
                return Shape::StringyUnion(values);
            }
            return Shape::UntaggedUnion(any_of.clone());
        }
        if let Some(values) = obj.get("enum").and_then(Value::as_array) {
            if values.iter().all(Value::is_string) {
                return Shape::StringEnum(
                    values
                        .iter()
                        .map(|v| v.as_str().unwrap().to_string())
                        .collect(),
                );
            }
            let prim = if values.iter().all(|v| v.as_i64().is_some()) {
                "i64"
            } else {
                "f64"
            };
            return Shape::NumberLiteral(prim);
        }
        match obj.get("type").and_then(Value::as_str) {
            Some("string") => Shape::Prim("String"),
            Some("integer") => Shape::Prim("i64"),
            Some("number") => Shape::Prim("f64"),
            Some("boolean") => Shape::Prim("bool"),
            Some("null") => Shape::Null,
            Some("array") => Shape::Array(obj.get("items").cloned().unwrap_or(Value::Bool(true))),
            Some("object") => {
                if obj.contains_key("properties") {
                    Shape::Struct
                } else if let Some(ap) = obj.get("additionalProperties") {
                    if ap.is_object() {
                        Shape::Record(ap.clone())
                    } else {
                        Shape::BareObject
                    }
                } else {
                    Shape::BareObject
                }
            }
            _ => Shape::Opaque,
        }
    }

    /// A union is "tagged" when every member is an inline object (or a ref to
    /// a struct def) sharing a required single-value string enum property with
    /// distinct values. Only inline-member unions are emitted internally
    /// tagged; ref members force the untagged representation (their structs
    /// declare the tag field themselves).
    fn detect_tagged(&self, members: &[Value]) -> Option<(String, Vec<(String, Value)>)> {
        let objs: Vec<&serde_json::Map<String, Value>> = members
            .iter()
            .map(|m| {
                m.as_object()
                    .filter(|o| o.get("type").and_then(Value::as_str) == Some("object"))
            })
            .collect::<Option<Vec<_>>>()?;
        let first_props = objs.first()?.get("properties")?.as_object()?;
        'candidates: for key in first_props.keys() {
            let mut literals = Vec::new();
            let mut seen = HashSet::new();
            for o in &objs {
                let props = o.get("properties")?.as_object()?;
                let required = o
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|r| r.iter().filter_map(Value::as_str).collect::<HashSet<_>>())
                    .unwrap_or_default();
                let Some(lit) = props.get(key).and_then(single_string_literal) else {
                    continue 'candidates;
                };
                if !required.contains(key.as_str()) || !seen.insert(lit.clone()) {
                    continue 'candidates;
                }
                literals.push(lit);
            }
            let pairs = literals
                .into_iter()
                .zip(members.iter().cloned())
                .collect::<Vec<_>>();
            return Some((key.clone(), pairs));
        }
        None
    }

    /// All members decode from a JSON string: string prims, string enums, or
    /// refs to such defs. Returns the union of known literal values.
    fn all_stringy(&self, members: &[Value]) -> Option<Vec<String>> {
        let mut values = Vec::new();
        let mut any_literal = false;
        for m in members {
            let resolved = self.resolve(m)?;
            let obj = resolved.as_object()?;
            if let Some(en) = obj.get("enum").and_then(Value::as_array) {
                if !en.iter().all(Value::is_string) {
                    return None;
                }
                any_literal = true;
                values.extend(en.iter().map(|v| v.as_str().unwrap().to_string()));
            } else if obj.get("type").and_then(Value::as_str) != Some("string") {
                return None;
            }
        }
        if any_literal { Some(values) } else { None }
    }

    fn resolve(&self, schema: &Value) -> Option<Value> {
        let obj = schema.as_object()?;
        if let Some(r) = obj.get("$ref").and_then(Value::as_str) {
            let name = ref_name(r).ok()?;
            return self.resolve(self.defs.get(&name)?);
        }
        Some(schema.clone())
    }

    // ---- emitters -----------------------------------------------------------

    fn emit_string_enum(&self, ident: &str, values: &[String]) -> Result<String> {
        let mut out = String::new();
        let default_derive = if values.len() == 1 { ", Default" } else { "" };
        writeln!(
            out,
            "#[derive(Debug, Clone, PartialEq, Eq, Hash{default_derive}, Serialize, Deserialize)]\npub enum {ident} {{"
        )?;
        let mut used = HashSet::new();
        used.insert("Unknown".to_string());
        for (i, v) in values.iter().enumerate() {
            let mut variant = pascal(v);
            while !used.insert(variant.clone()) {
                variant.push('X');
            }
            if values.len() == 1 && i == 0 {
                writeln!(out, "    #[default]")?;
            }
            writeln!(out, "    #[serde(rename = \"{}\")]\n    {variant},", esc(v))?;
        }
        writeln!(
            out,
            "    /// Forward compatibility: a literal this build does not know.\n    #[serde(untagged)]\n    Unknown(String),\n}}\n"
        )?;
        Ok(out)
    }

    fn emit_struct(&self, ident: &str, schema: &Value, ctx: &mut EmitCtx) -> Result<String> {
        let mut out = String::new();
        writeln!(
            out,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub struct {ident} {{"
        )?;
        out.push_str(&self.emit_fields(schema, ctx, ident, "    ", "pub ")?);
        writeln!(out, "}}\n")?;
        Ok(out)
    }

    fn emit_fields(
        &self,
        schema: &Value,
        ctx: &mut EmitCtx,
        type_ident: &str,
        indent: &str,
        vis: &str,
    ) -> Result<String> {
        let mut out = String::new();
        let obj = schema.as_object().context("struct schema is object")?;
        let props = obj
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let required: HashSet<String> = obj
            .get("required")
            .and_then(Value::as_array)
            .map(|r| {
                r.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        for (json_name, prop) in &props {
            let is_required = required.contains(json_name);
            let (nullable, inner) = match prop.as_object().and_then(|o| {
                o.get("anyOf")
                    .and_then(Value::as_array)
                    .filter(|a| a.len() == 2)
                    .and_then(|a| nullable_inner(a))
            }) {
                Some(inner) => (true, inner),
                None => (false, prop.clone()),
            };
            let field_hint = format!("{type_ident}{}", pascal(json_name));
            let base = self.rust_type_named(&inner, ctx, true, &field_hint)?;
            let (ty, attr) = match (is_required, nullable) {
                (true, false) => (base, String::new()),
                (true, true) => (format!("Option<{base}>"), String::new()),
                (false, false) => (
                    format!("Option<{base}>"),
                    "#[serde(default, skip_serializing_if = \"Option::is_none\")]".to_string(),
                ),
                (false, true) => (
                    format!("Option<Option<{base}>>"),
                    "#[serde(default, skip_serializing_if = \"Option::is_none\", with = \"crate::support::double_option\")]"
                        .to_string(),
                ),
            };
            let field = field_ident(json_name);
            if !attr.is_empty() {
                writeln!(out, "{indent}{attr}")?;
            }
            if field_needs_rename(json_name, &field) {
                writeln!(out, "{indent}#[serde(rename = \"{}\")]", esc(json_name))?;
            }
            writeln!(out, "{indent}{vis}{field}: {ty},")?;
        }
        Ok(out)
    }

    fn emit_tagged_union(
        &self,
        ident: &str,
        disc: &str,
        members: &[(String, Value)],
        ctx: &mut EmitCtx,
    ) -> Result<String> {
        let mut out = String::new();
        writeln!(
            out,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n#[serde(tag = \"{}\")]\npub enum {ident} {{",
            esc(disc)
        )?;
        let mut used = HashSet::new();
        used.insert("Unknown".to_string());
        for (literal, member) in members {
            let mut variant = pascal(literal);
            while !used.insert(variant.clone()) {
                variant.push('X');
            }
            // struct variant with the member's fields minus the discriminant
            let mut member = member.clone();
            if let Some(props) = member
                .as_object_mut()
                .and_then(|o| o.get_mut("properties"))
                .and_then(Value::as_object_mut)
            {
                props.remove(disc);
            }
            writeln!(out, "    #[serde(rename = \"{}\")]", esc(literal))?;
            writeln!(out, "    {variant} {{")?;
            let hint = format!("{ident}{variant}");
            out.push_str(&self.emit_fields(&member, ctx, &hint, "        ", "")?);
            writeln!(out, "    }},")?;
        }
        writeln!(
            out,
            "    /// Forward compatibility: an event kind this build does not know.\n    #[serde(untagged)]\n    Unknown(serde_json::Value),\n}}\n"
        )?;
        Ok(out)
    }

    fn emit_untagged_union(
        &self,
        ident: &str,
        members: &[Value],
        ctx: &mut EmitCtx,
    ) -> Result<String> {
        let mut out = String::new();
        writeln!(
            out,
            "#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n#[serde(untagged)]\npub enum {ident} {{"
        )?;
        let mut used = HashSet::new();
        used.insert("Unknown".to_string());
        for (i, member) in members.iter().enumerate() {
            let obj = member.as_object();
            // ref member → newtype variant of the target type
            if let Some(r) = obj.and_then(|o| o.get("$ref")).and_then(Value::as_str) {
                let target = ref_name(r)?;
                let target_ident = self.ident(&target);
                let mut variant = target_ident.clone();
                while !used.insert(variant.clone()) {
                    variant.push('X');
                }
                let content = if self.needs_box(&ctx.owner, &target) {
                    format!("Box<{target_ident}>")
                } else {
                    target_ident
                };
                writeln!(out, "    {variant}({content}),")?;
                continue;
            }
            // inline member → name by discriminant literal when present
            let lit = obj.and_then(|o| {
                o.get("properties")?.as_object()?.iter().find_map(|(k, v)| {
                    if matches!(k.as_str(), "kind" | "type" | "_tag" | "status") {
                        single_string_literal(v)
                    } else {
                        None
                    }
                })
            });
            let mut variant = lit.map(|l| pascal(&l)).unwrap_or_else(|| format!("V{i}"));
            while !used.insert(variant.clone()) {
                variant.push('X');
            }
            let hint = format!("{ident}{variant}");
            let is_object_member = obj.is_some_and(|o| {
                o.get("type").and_then(Value::as_str) == Some("object")
                    && o.contains_key("properties")
            });
            if is_object_member {
                writeln!(out, "    {variant} {{")?;
                out.push_str(&self.emit_fields(member, ctx, &hint, "        ", "")?);
                writeln!(out, "    }},")?;
            } else {
                let t = self.rust_type_named(member, ctx, true, &hint)?;
                writeln!(out, "    {variant}({t}),")?;
            }
        }
        writeln!(
            out,
            "    /// Forward compatibility: a member this build does not know.\n    Unknown(serde_json::Value),\n}}\n"
        )?;
        Ok(out)
    }

    // ---- inline type mapping ------------------------------------------------

    fn rust_type(&self, schema: &Value, ctx: &mut EmitCtx, direct: bool) -> Result<String> {
        self.rust_type_named(schema, ctx, direct, "")
    }

    fn rust_type_named(
        &self,
        schema: &Value,
        ctx: &mut EmitCtx,
        direct: bool,
        hint: &str,
    ) -> Result<String> {
        let Some(obj) = schema.as_object() else {
            return Ok("serde_json::Value".to_string());
        };
        if let Some(r) = obj.get("$ref").and_then(Value::as_str) {
            let target = ref_name(r)?;
            let ident = self.ident(&target);
            if direct && self.needs_box(&ctx.owner, &target) {
                return Ok(format!("Box<{ident}>"));
            }
            return Ok(ident);
        }
        if obj.contains_key("not") {
            return Ok("serde_json::Value".to_string());
        }
        if let Some(any_of) = obj.get("anyOf").and_then(Value::as_array) {
            if is_empty_payload(any_of) {
                return Ok("serde_json::Value".to_string());
            }
            if let Some(inner) = effect_option_inner(any_of) {
                let t = self.rust_type_named(&inner, ctx, direct, hint)?;
                return Ok(format!("EffectOption<{t}>"));
            }
            if is_duration(any_of) {
                return Ok("DurationMillis".to_string());
            }
            if any_of.len() == 2
                && let Some(inner) = nullable_inner(any_of)
            {
                let t = self.rust_type_named(&inner, ctx, direct, hint)?;
                return Ok(format!("Option<{t}>"));
            }
            // hoist a named union type
            let name = ctx.hoist_name(hint);
            let decl = if let Some((disc, members)) = self.detect_tagged(any_of) {
                self.emit_tagged_union(&name, &disc, &members, ctx)?
            } else if let Some(values) = self.all_stringy(any_of) {
                self.emit_string_enum(&name, &values)?
            } else {
                self.emit_untagged_union(&name, any_of, ctx)?
            };
            ctx.hoists.push_str(&decl);
            return Ok(name);
        }
        if let Some(values) = obj.get("enum").and_then(Value::as_array) {
            if values.iter().all(Value::is_string) {
                let strings: Vec<String> = values
                    .iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect();
                let name = ctx.hoist_name(hint);
                let decl = self.emit_string_enum(&name, &strings)?;
                ctx.hoists.push_str(&decl);
                return Ok(name);
            }
            let prim = if values.iter().all(|v| v.as_i64().is_some()) {
                "i64"
            } else {
                "f64"
            };
            return Ok(prim.to_string());
        }
        match obj.get("type").and_then(Value::as_str) {
            Some("string") => Ok("String".to_string()),
            Some("integer") => Ok("i64".to_string()),
            Some("number") => Ok("f64".to_string()),
            Some("boolean") => Ok("bool".to_string()),
            Some("null") => Ok("()".to_string()),
            Some("array") => {
                let items = obj.get("items").cloned().unwrap_or(Value::Bool(true));
                let t = self.rust_type_named(&items, ctx, false, hint)?;
                Ok(format!("Vec<{t}>"))
            }
            Some("object") => {
                if obj.contains_key("properties") {
                    let name = ctx.hoist_name(hint);
                    let decl = self.emit_struct(&name, schema, ctx)?;
                    ctx.hoists.push_str(&decl);
                    Ok(name)
                } else if let Some(ap) = obj.get("additionalProperties").filter(|v| v.is_object()) {
                    let ap = ap.clone();
                    let t = self.rust_type_named(&ap, ctx, false, hint)?;
                    Ok(format!("BTreeMap<String, {t}>"))
                } else {
                    Ok("serde_json::Map<String, serde_json::Value>".to_string())
                }
            }
            _ => Ok("serde_json::Value".to_string()),
        }
    }

    // ---- methods + fixtures ---------------------------------------------------

    fn emit_methods(&self, methods: &[Value]) -> Result<String> {
        let mut out = String::new();
        out.push_str(
            "/// Typed table of every WebSocket RPC method (see `RpcMethod`).\npub mod methods {\n    use crate::support::RpcMethod;\n    use serde::{Deserialize, Serialize};\n\n",
        );
        let def_idents: HashSet<String> = self.idents.values().cloned().collect();
        for m in methods {
            let tag = m["tag"].as_str().context("method tag")?;
            let stream = m["stream"].as_bool().context("method stream")?;
            let payload = self.ident(&ref_name(m["payload"].as_str().unwrap())?);
            let success = self.ident(&ref_name(m["success"].as_str().unwrap())?);
            let errors = m["errors"]
                .as_array()
                .context("errors")?
                .iter()
                .map(|e| ref_name(e.as_str().unwrap()).map(|n| self.ident(&n)))
                .collect::<Result<Vec<_>>>()?;
            let base = pascal(tag);
            let mut err_ident = format!("{base}Error");
            while def_idents.contains(&err_ident) {
                err_ident.push('X');
            }
            writeln!(out, "    pub struct {base};\n")?;
            writeln!(out, "    impl RpcMethod for {base} {{")?;
            writeln!(out, "        const TAG: &'static str = \"{}\";", esc(tag))?;
            writeln!(out, "        const STREAM: bool = {stream};")?;
            writeln!(out, "        type Payload = super::{payload};")?;
            writeln!(out, "        type Success = super::{success};")?;
            writeln!(out, "        type Error = {err_ident};")?;
            writeln!(out, "    }}\n")?;
            writeln!(
                out,
                "    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n    #[serde(untagged)]\n    pub enum {err_ident} {{"
            )?;
            let mut seen = HashSet::new();
            for e in &errors {
                if seen.insert(e.clone()) {
                    writeln!(out, "        {e}(super::{e}),")?;
                }
            }
            writeln!(out, "        Unknown(serde_json::Value),\n    }}\n")?;
        }
        out.push_str("}\n\n");
        Ok(out)
    }

    fn emit_fixture_tests(&self, fixtures_dir: &std::path::Path) -> Result<String> {
        let mut out = String::new();
        out.push_str("#[cfg(test)]\nmod fixture_tests {\n    use crate::support::assert_fixture_roundtrip;\n\n");
        let mut entries: Vec<_> = std::fs::read_dir(fixtures_dir)
            .context("read fixtures dir")?
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.ends_with(".json") {
                continue;
            }
            let fixture: Value = serde_json::from_str(&std::fs::read_to_string(entry.path())?)?;
            let schema_name = fixture["schema"].as_str().context("fixture schema")?;
            if !self.defs.contains_key(schema_name) {
                bail!("fixture {file_name} references unknown def {schema_name}");
            }
            let ident = self.ident(schema_name);
            let fn_name = snake(schema_name);
            writeln!(
                out,
                "    #[test]\n    fn {fn_name}() {{\n        assert_fixture_roundtrip::<super::{ident}>(include_str!(\"../fixtures/{file_name}\"));\n    }}\n"
            )?;
        }
        out.push_str("}\n");
        Ok(out)
    }
}

struct EmitCtx {
    owner: String,
    hoists: String,
    hoist_names: HashSet<String>,
}

impl EmitCtx {
    fn hoist_name(&mut self, hint: &str) -> String {
        let base = if hint.is_empty() {
            format!("{}Inline", pascal(&self.owner))
        } else {
            hint.to_string()
        };
        let mut name = base.clone();
        let mut n = 2;
        while !self.hoist_names.insert(name.clone()) {
            name = format!("{base}{n}");
            n += 1;
        }
        name
    }
}

enum Shape {
    EmptyPayload,
    Null,
    Never,
    Prim(&'static str),
    StringEnum(Vec<String>),
    NumberLiteral(&'static str),
    Nullable(Value),
    EffectOption(Value),
    Duration,
    Array(Value),
    Record(Value),
    BareObject,
    Struct,
    TaggedUnion(String, Vec<(String, Value)>),
    StringyUnion(Vec<String>),
    UntaggedUnion(Vec<Value>),
    Opaque,
}

// ---- schema predicates ------------------------------------------------------

fn ref_name(r: &str) -> Result<String> {
    r.strip_prefix("#/$defs/")
        .map(String::from)
        .with_context(|| format!("unsupported ref {r}"))
}

fn single_string_literal(schema: &Value) -> Option<String> {
    let en = schema.as_object()?.get("enum")?.as_array()?;
    if en.len() == 1 {
        en[0].as_str().map(String::from)
    } else {
        None
    }
}

/// `Schema.Struct({})` payloads render as `anyOf [{type: object}, {type: array}]`.
fn is_empty_payload(members: &[Value]) -> bool {
    members.len() == 2
        && members.iter().any(|m| {
            m.as_object().is_some_and(|o| {
                o.get("type").and_then(Value::as_str) == Some("object")
                    && !o.contains_key("properties")
            })
        })
        && members.iter().any(|m| {
            m.as_object()
                .is_some_and(|o| o.get("type").and_then(Value::as_str) == Some("array"))
        })
}

fn nullable_inner(members: &[Value]) -> Option<Value> {
    let null_pos = members.iter().position(|m| {
        m.as_object()
            .is_some_and(|o| o.get("type").and_then(Value::as_str) == Some("null"))
    })?;
    if members.len() != 2 {
        return None;
    }
    Some(members[1 - null_pos].clone())
}

/// `Schema.Option(T)`: anyOf of `{_tag: Some, value}` and `{_tag: None}`.
fn effect_option_inner(members: &[Value]) -> Option<Value> {
    if members.len() != 2 {
        return None;
    }
    let mut some_value = None;
    let mut has_none = false;
    for m in members {
        let props = m.as_object()?.get("properties")?.as_object()?;
        match single_string_literal(props.get("_tag")?)?.as_str() {
            "Some" => some_value = Some(props.get("value")?.clone()),
            "None" => has_none = true,
            _ => return None,
        }
    }
    if has_none { some_value } else { None }
}

/// `Schema.DurationFromMillis`: number plus NaN/Infinity string sentinels.
fn is_duration(members: &[Value]) -> bool {
    let mut has_number = false;
    let mut sentinels = 0;
    for m in members {
        let Some(obj) = m.as_object() else {
            return false;
        };
        if obj.get("type").and_then(Value::as_str) == Some("number") {
            has_number = true;
        } else if let Some(lit) = single_string_literal(m) {
            if matches!(lit.as_str(), "NaN" | "Infinity" | "-Infinity") {
                sentinels += 1;
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
    has_number && sentinels > 0
}

/// Direct (non-heap) ref edges: recursion through Vec/BTreeMap is finite, so
/// `items` and `additionalProperties` subtrees are skipped.
fn collect_direct_refs(schema: &Value, out: &mut HashSet<String>) {
    match schema {
        Value::Object(obj) => {
            for (key, value) in obj {
                match key.as_str() {
                    "$ref" => {
                        if let Some(r) = value.as_str()
                            && let Ok(name) = ref_name(r)
                        {
                            out.insert(name);
                        }
                    }
                    "items" | "additionalProperties" | "enum" | "not" => {}
                    _ => collect_direct_refs(value, out),
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_direct_refs(item, out);
            }
        }
        _ => {}
    }
}

// ---- naming -----------------------------------------------------------------

const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in", "let", "loop",
    "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return", "static",
    "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use", "virtual",
    "where", "while", "yield",
];
const UNRAWABLE: &[&str] = &["self", "Self", "super", "crate"];

fn pascal(s: &str) -> String {
    let mut out = String::new();
    let mut upper_next = true;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            if upper_next {
                out.extend(ch.to_uppercase());
                upper_next = false;
            } else {
                out.push(ch);
            }
        } else {
            upper_next = true;
        }
    }
    if out.is_empty() {
        out.push_str("Unnamed");
    }
    if out.chars().next().unwrap().is_ascii_digit() {
        out.insert(0, 'N');
    }
    out
}

fn snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() {
            if ch.is_ascii_uppercase() {
                if prev_lower {
                    out.push('_');
                }
                out.extend(ch.to_lowercase());
                prev_lower = false;
            } else {
                out.push(ch);
                prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
            }
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
            prev_lower = false;
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        return "field".to_string();
    }
    if out.chars().next().unwrap().is_ascii_digit() {
        return format!("n{out}");
    }
    out
}

fn field_ident(json_name: &str) -> String {
    let base = snake(json_name);
    if UNRAWABLE.contains(&base.as_str()) {
        return format!("{base}_");
    }
    if KEYWORDS.contains(&base.as_str()) {
        return format!("r#{base}");
    }
    base
}

fn field_needs_rename(json_name: &str, field_ident: &str) -> bool {
    field_ident.strip_prefix("r#").unwrap_or(field_ident) != json_name
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
