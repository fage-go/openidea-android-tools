use openidea_plugin::Error;
use std::collections::BTreeMap;

type Map = Vec<(u16, Value)>;
type Names = BTreeMap<u16, String>;

#[derive(Debug)]
pub(super) struct Node {
    pub class: String,
    pub bounds: Option<[f64; 4]>,
    pub properties: Vec<(String, String)>,
    pub children: Vec<Node>,
}

enum Value {
    Boolean(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Map(Map),
}

struct Decoder<'a> {
    remaining: &'a [u8],
    values: usize,
}

fn invalid() -> Error {
    "Android 返回了不完整或不支持的 V2 布局数据".into()
}

impl<'a> Decoder<'a> {
    fn bytes(&mut self, size: usize) -> Result<&'a [u8], Error> {
        let (bytes, rest) = self.remaining.split_at_checked(size).ok_or_else(invalid)?;
        self.remaining = rest;
        Ok(bytes)
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    fn key(&mut self) -> Result<u16, Error> {
        if self.fixed::<1>()?[0] != b'S' {
            return Err(invalid());
        }
        Ok(u16::from_be_bytes(self.fixed()?))
    }

    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        self.values += 1;
        if depth > 128 || self.values > 500_000 {
            return Err("V2 布局树过大，无法安全解析".into());
        }
        match self.fixed::<1>()?[0] {
            b'Z' => Ok(Value::Boolean(self.fixed::<1>()?[0] != 0)),
            b'B' => Ok(Value::Int(i8::from_be_bytes(self.fixed()?).into())),
            b'S' => Ok(Value::Int(i16::from_be_bytes(self.fixed()?).into())),
            b'I' => Ok(Value::Int(i32::from_be_bytes(self.fixed()?).into())),
            b'J' => Ok(Value::Int(i64::from_be_bytes(self.fixed()?))),
            b'F' => Ok(Value::Float(f32::from_be_bytes(self.fixed()?).into())),
            b'D' => Ok(Value::Float(f64::from_be_bytes(self.fixed()?))),
            b'R' => {
                let len = u16::from_be_bytes(self.fixed()?) as usize;
                Ok(Value::Text(
                    String::from_utf8_lossy(self.bytes(len)?).into_owned(),
                ))
            }
            b'M' => {
                let mut map = Vec::new();
                loop {
                    let key = self.key()?;
                    if key == 0 {
                        break;
                    }
                    map.push((key, self.value(depth + 1)?));
                }
                Ok(Value::Map(map))
            }
            _ => Err(invalid()),
        }
    }
}

// Android's ViewHierarchyEncoder writes window properties, the root object, and the name table.
pub(super) fn decode(bytes: &[u8]) -> Result<Node, Error> {
    let mut decoder = Decoder {
        remaining: bytes,
        values: 0,
    };
    let mut window = Vec::new();
    while decoder.remaining.first() == Some(&b'S') {
        window.push((decoder.key()?, decoder.value(0)?));
    }
    let Value::Map(root) = decoder.value(0)? else {
        return Err(invalid());
    };
    let Value::Map(table) = decoder.value(0)? else {
        return Err(invalid());
    };
    if !decoder.remaining.is_empty() {
        return Err(invalid());
    }
    let names: Names = table
        .into_iter()
        .map(|(key, value)| match value {
            Value::Text(name) => Ok((key, name)),
            _ => Err(invalid()),
        })
        .collect::<Result<_, _>>()?;
    let window = Properties {
        entries: &window,
        names: &names,
    };
    let offset = Transform::translation(
        window.number("window:left", 0.0),
        window.number("window:top", 0.0),
    );
    node(root, &names, offset, true)
}

struct Properties<'a> {
    entries: &'a Map,
    names: &'a Names,
}

impl Properties<'_> {
    fn get(&self, name: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(key, _)| self.names.get(key).is_some_and(|s| s == name))
            .map(|(_, value)| value)
    }

    fn number(&self, name: &str, default: f64) -> f64 {
        match self.get(name) {
            Some(Value::Int(i)) => *i as f64,
            Some(Value::Float(f)) if f.is_finite() => *f,
            _ => default,
        }
    }
}

#[derive(Clone, Copy)]
struct Transform {
    xx: f64,
    xy: f64,
    yx: f64,
    yy: f64,
    x: f64,
    y: f64,
}

impl Transform {
    fn translation(x: f64, y: f64) -> Self {
        Self {
            xx: 1.0,
            xy: 0.0,
            yx: 0.0,
            yy: 1.0,
            x,
            y,
        }
    }

    fn then(self, local: Self) -> Self {
        let [x, y] = self.point(local.x, local.y);
        Self {
            xx: self.xx * local.xx + self.xy * local.yx,
            xy: self.xx * local.xy + self.xy * local.yy,
            yx: self.yx * local.xx + self.yy * local.yx,
            yy: self.yx * local.xy + self.yy * local.yy,
            x,
            y,
        }
    }

    fn point(self, x: f64, y: f64) -> [f64; 2] {
        [
            self.x + self.xx * x + self.xy * y,
            self.y + self.yx * x + self.yy * y,
        ]
    }

    fn bounds(self, width: f64, height: f64) -> Option<[f64; 4]> {
        let corners = [
            self.point(0.0, 0.0),
            self.point(width, 0.0),
            self.point(0.0, height),
            self.point(width, height),
        ];
        let [x, y] = corners[0];
        let bounds = corners
            .iter()
            .fold([x, y, x, y], |[left, top, right, bottom], &[x, y]| {
                [left.min(x), top.min(y), right.max(x), bottom.max(y)]
            });
        bounds.iter().all(|n| n.is_finite()).then_some(bounds)
    }
}

fn node(
    entries: Map,
    names: &Names,
    parent: Transform,
    parent_visible: bool,
) -> Result<Node, Error> {
    let props = Properties {
        entries: &entries,
        names,
    };
    let Some(Value::Text(class)) = props.get("meta:__name__") else {
        return Err(invalid());
    };
    let class = class.clone();
    let left = props.number("layout:left", 0.0);
    let top = props.number("layout:top", 0.0);
    let width = props.number("layout:width", props.number("layout:right", left) - left);
    let height = props.number("layout:height", props.number("layout:bottom", top) - top);
    let tx = props.number("drawing:translationX", 0.0);
    let ty = props.number("drawing:translationY", 0.0);
    let sx = props.number("drawing:scaleX", 1.0);
    let sy = props.number("drawing:scaleY", 1.0);
    let px = props.number("drawing:pivotX", width / 2.0);
    let py = props.number("drawing:pivotY", height / 2.0);
    let (sin, cos) = props.number("drawing:rotation", 0.0).to_radians().sin_cos();
    let transform = parent
        .then(Transform::translation(left + tx + px, top + ty + py))
        .then(Transform {
            xx: cos * sx,
            xy: -sin * sy,
            yx: sin * sx,
            yy: cos * sy,
            x: 0.0,
            y: 0.0,
        })
        .then(Transform::translation(-px, -py));
    let visible = parent_visible
        && props.number("misc:visibility", 0.0) == 0.0
        && props.number("drawing:alpha", 1.0) > 0.0;
    let bounds = if visible && width > 0.0 && height > 0.0 {
        transform.bounds(width, height)
    } else {
        None
    };
    let child_transform = transform.then(Transform::translation(
        -props.number("scrolling:scrollX", 0.0),
        -props.number("scrolling:scrollY", 0.0),
    ));

    let mut children = Vec::new();
    let mut properties = Vec::new();
    for (key, value) in entries {
        let Some(name) = names.get(&key) else {
            continue;
        };
        if let Some(index) = name
            .strip_prefix("meta:__child__")
            .and_then(|n| n.parse::<usize>().ok())
        {
            let Value::Map(child) = value else {
                return Err(invalid());
            };
            children.push((index, child));
        } else {
            property(&mut properties, name.clone(), value, names);
        }
    }
    children.sort_by_key(|(index, _)| *index);
    properties.sort_by(|a, b| a.0.cmp(&b.0));
    let children = children
        .into_iter()
        .map(|(_, child)| node(child, names, child_transform, visible))
        .collect::<Result<_, _>>()?;
    Ok(Node {
        class,
        bounds,
        properties,
        children,
    })
}

fn property(out: &mut Vec<(String, String)>, name: String, value: Value, names: &Names) {
    let value = match value {
        Value::Boolean(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::Text(value) => value,
        Value::Map(entries) => {
            for (key, value) in entries {
                if let Some(field) = names.get(&key) {
                    property(out, format!("{name}.{field}"), value, names);
                }
            }
            return;
        }
    };
    out.push((name, value));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_window_offsets_and_rejects_a_truncated_name_table() {
        let mut bytes =
            b"S\0\x01I\0\0\0\x10MS\0\x02R\0\x04ViewS\0\x03I\0\0\0\x20S\0\x04I\0\0\0\x10S\0\0M"
                .to_vec();
        for (key, name) in [
            (1u16, "window:left"),
            (2, "meta:__name__"),
            (3, "layout:width"),
            (4, "layout:height"),
        ] {
            bytes.push(b'S');
            bytes.extend(key.to_be_bytes());
            bytes.push(b'R');
            bytes.extend((name.len() as u16).to_be_bytes());
            bytes.extend(name.as_bytes());
        }
        bytes.extend(b"S\0\0");
        let view = decode(&bytes).unwrap();
        assert_eq!(view.class, "View");
        assert_eq!(view.bounds, Some([16.0, 0.0, 48.0, 16.0]));
        bytes.pop();
        assert!(decode(&bytes).is_err());
    }
}
