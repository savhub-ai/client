use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Parse a KDL string into a deserializable type.
pub fn parse_kdl<T: DeserializeOwned>(content: &str) -> Result<T, String> {
    let doc: kdl::KdlDocument = content
        .parse()
        .map_err(|e| format!("KDL parse error: {e}"))?;
    let json_value = kdl_document_to_json(&doc);
    serde_json::from_value(json_value).map_err(|e| format!("deserialization error: {e}"))
}

/// Serialize a value to a KDL string.
pub fn to_kdl_string<T: Serialize>(value: &T) -> Result<String, String> {
    let json = serde_json::to_value(value).map_err(|e| format!("serialization error: {e}"))?;
    let doc = json_to_kdl_document(&json);
    Ok(doc.to_string())
}

pub fn is_kdl_path(path: &std::path::Path) -> bool {
    path.extension().map_or(false, |ext| ext == "kdl")
}

// ── KDL → JSON ──

fn kdl_document_to_json(doc: &kdl::KdlDocument) -> Value {
    let mut map = Map::new();
    for node in doc.nodes() {
        let key = node.name().value().to_string();
        let value = kdl_node_to_json(node);
        if let Some(existing) = map.get_mut(&key) {
            match existing {
                Value::Array(arr) => arr.push(value),
                _ => {
                    let prev = existing.clone();
                    *existing = Value::Array(vec![prev, value]);
                }
            }
        } else {
            map.insert(key, value);
        }
    }
    Value::Object(map)
}

fn kdl_node_to_json(node: &kdl::KdlNode) -> Value {
    let has_children = node.children().map_or(false, |c| !c.nodes().is_empty());
    let args: Vec<_> = node
        .entries()
        .iter()
        .filter(|e| e.name().is_none())
        .collect();
    let props: Vec<_> = node
        .entries()
        .iter()
        .filter(|e| e.name().is_some())
        .collect();

    if has_children {
        let children_doc = node.children().unwrap();
        let all_dash = children_doc.nodes().iter().all(|n| n.name().value() == "-");
        if all_dash && !children_doc.nodes().is_empty() {
            return Value::Array(
                children_doc
                    .nodes()
                    .iter()
                    .map(|n| dash_to_json(n))
                    .collect(),
            );
        }
        let mut obj = match kdl_document_to_json(children_doc) {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        for prop in &props {
            obj.insert(
                prop.name().unwrap().value().to_string(),
                kdl_val(prop.value()),
            );
        }
        return Value::Object(obj);
    }
    if args.len() == 1 && props.is_empty() {
        return kdl_val(args[0].value());
    }
    if args.len() > 1 && props.is_empty() {
        return Value::Array(args.iter().map(|a| kdl_val(a.value())).collect());
    }
    if !props.is_empty() {
        let mut obj = Map::new();
        for prop in &props {
            obj.insert(
                prop.name().unwrap().value().to_string(),
                kdl_val(prop.value()),
            );
        }
        return Value::Object(obj);
    }
    Value::Null
}

fn dash_to_json(node: &kdl::KdlNode) -> Value {
    let args: Vec<_> = node
        .entries()
        .iter()
        .filter(|e| e.name().is_none())
        .collect();
    let props: Vec<_> = node
        .entries()
        .iter()
        .filter(|e| e.name().is_some())
        .collect();
    let has_children = node.children().map_or(false, |c| !c.nodes().is_empty());
    if has_children {
        let mut obj = match kdl_document_to_json(node.children().unwrap()) {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        for prop in &props {
            obj.insert(
                prop.name().unwrap().value().to_string(),
                kdl_val(prop.value()),
            );
        }
        return Value::Object(obj);
    }
    if args.len() == 1 && props.is_empty() {
        return kdl_val(args[0].value());
    }
    if !props.is_empty() {
        let mut obj = Map::new();
        for prop in props {
            obj.insert(
                prop.name().unwrap().value().to_string(),
                kdl_val(prop.value()),
            );
        }
        return Value::Object(obj);
    }
    Value::Null
}

fn kdl_val(value: &kdl::KdlValue) -> Value {
    match value {
        kdl::KdlValue::String(s) => Value::String(s.clone()),
        kdl::KdlValue::Integer(i) => Value::Number((*i as i64).into()),
        kdl::KdlValue::Float(f) => serde_json::Number::from_f64(*f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        kdl::KdlValue::Bool(b) => Value::Bool(*b),
        kdl::KdlValue::Null => Value::Null,
    }
}

// ── JSON → KDL ──

fn json_to_kdl_document(value: &Value) -> kdl::KdlDocument {
    let mut doc = kdl::KdlDocument::new();
    if let Value::Object(map) = value {
        for (key, val) in map {
            doc.nodes_mut().push(json_to_kdl_node(key, val));
        }
    }
    doc
}

fn json_to_kdl_node(name: &str, value: &Value) -> kdl::KdlNode {
    let mut node = kdl::KdlNode::new(name);
    match value {
        Value::Object(map) => {
            let mut children = kdl::KdlDocument::new();
            for (key, val) in map {
                children.nodes_mut().push(json_to_kdl_node(key, val));
            }
            node.set_children(children);
        }
        Value::Array(arr) => {
            let mut children = kdl::KdlDocument::new();
            for item in arr {
                children.nodes_mut().push(json_value_to_dash_node(item));
            }
            node.set_children(children);
        }
        Value::String(s) => {
            node.push(kdl::KdlEntry::new(kdl::KdlValue::String(s.clone())));
        }
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                node.push(kdl::KdlEntry::new(kdl::KdlValue::Integer(i as i128)));
            } else if let Some(f) = n.as_f64() {
                node.push(kdl::KdlEntry::new(kdl::KdlValue::Float(f)));
            }
        }
        Value::Bool(b) => {
            node.push(kdl::KdlEntry::new(kdl::KdlValue::Bool(*b)));
        }
        Value::Null => {
            node.push(kdl::KdlEntry::new(kdl::KdlValue::Null));
        }
    }
    node
}

fn json_value_to_dash_node(value: &Value) -> kdl::KdlNode {
    let mut node = kdl::KdlNode::new("-");
    match value {
        Value::Object(map) => {
            // If all values are scalars, use properties on the dash node
            let all_scalar = map.values().all(|v| {
                matches!(
                    v,
                    Value::String(_) | Value::Number(_) | Value::Bool(_) | Value::Null
                )
            });
            if all_scalar {
                for (key, val) in map {
                    node.push(kdl::KdlEntry::new_prop(key.clone(), json_to_kdl_value(val)));
                }
            } else {
                let mut children = kdl::KdlDocument::new();
                for (key, val) in map {
                    children.nodes_mut().push(json_to_kdl_node(key, val));
                }
                node.set_children(children);
            }
        }
        other => {
            node.push(kdl::KdlEntry::new(json_to_kdl_value(other)));
        }
    }
    node
}

fn json_to_kdl_value(value: &Value) -> kdl::KdlValue {
    match value {
        Value::String(s) => kdl::KdlValue::String(s.clone()),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                kdl::KdlValue::Integer(i as i128)
            } else if let Some(f) = n.as_f64() {
                kdl::KdlValue::Float(f)
            } else {
                kdl::KdlValue::Null
            }
        }
        Value::Bool(b) => kdl::KdlValue::Bool(*b),
        _ => kdl::KdlValue::Null,
    }
}
