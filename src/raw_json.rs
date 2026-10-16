use quick_xml::Reader;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{XmlVersion, events::attributes::Attribute};
use serde_json::{Map, Value};

/// GPX 1.1 で複数置ける要素。1 つしか無くても配列にして、ファイルによって JSON の形が変わらないようにする
const ALWAYS_ARRAY: [&str; 7] = ["wpt", "rte", "rtept", "trk", "trkseg", "trkpt", "link"];

struct Node {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    text: String,
}

/// XML 全体を JSON に写す。属性は "@名前"、子要素を持つ要素のテキストは "#text" に入れる。
/// 値はすべて文字列のままにし、数値への変換や必須の値の確認はしない
pub fn convert(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;

    loop {
        let event = reader.read_event().map_err(|e| {
            format!(
                "XML を解析できません（{} バイト目付近）: {e}",
                reader.buffer_position()
            )
        })?;
        match event {
            Event::Start(e) => stack.push(node(&e)?),
            Event::Empty(e) => attach(node(&e)?, &mut stack, &mut root),
            Event::End(_) => {
                let finished = stack.pop().expect("終了タグは開始タグと対応している");
                attach(finished, &mut stack, &mut root);
            }
            Event::Text(t) => push_text(&mut stack, &t.xml10_content()),
            Event::CData(t) => push_text(&mut stack, &t),
            Event::GeneralRef(r) => {
                if let Some(c) = r.resolve_char_ref().map_err(|e| e.to_string())? {
                    push_text(&mut stack, c.encode_utf8(&mut [0; 4]));
                } else if let Some(s) = resolve_xml_entity(&r) {
                    push_text(&mut stack, s);
                } else {
                    return Err(format!("定義されていない実体参照です: &{};", &*r));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    let root = root.ok_or("XML の要素がありません")?;
    let mut doc = Map::new();
    doc.insert(root.name.clone(), value(root));
    let mut json =
        serde_json::to_string_pretty(&Value::Object(doc)).expect("文字列だけなので失敗しない");
    json.push('\n');
    Ok(json)
}

fn node(e: &BytesStart) -> Result<Node, String> {
    let attrs = e
        .attributes()
        .map(|a| {
            let a: Attribute = a.map_err(|err| err.to_string())?;
            let key = a.key.as_ref().to_string();
            let value = a
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|err| err.to_string())?;
            Ok((key, value.into_owned()))
        })
        .collect::<Result<_, String>>()?;
    Ok(Node {
        name: e.name().as_ref().to_string(),
        attrs,
        children: Vec::new(),
        text: String::new(),
    })
}

fn attach(node: Node, stack: &mut [Node], root: &mut Option<Node>) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(node),
        None => *root = Some(node),
    }
}

/// ルート要素の外のテキスト（空白など）は捨てる
fn push_text(stack: &mut [Node], text: &str) {
    if let Some(node) = stack.last_mut() {
        node.text.push_str(text);
    }
}

fn value(node: Node) -> Value {
    if node.attrs.is_empty() && node.children.is_empty() {
        return Value::String(node.text);
    }

    let mut map = Map::new();
    for (key, v) in node.attrs {
        map.insert(format!("@{key}"), Value::String(v));
    }
    // 子要素を持つ要素のテキストは、たいてい字下げの空白なので、空白だけなら出さない
    let text = node.text.trim();
    if !text.is_empty() {
        map.insert("#text".to_string(), Value::String(text.to_string()));
    }

    // 同じ名前の子要素は、最初に出てきた位置にまとめる
    let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
    for child in node.children {
        let name = child.name.clone();
        let v = value(child);
        match groups.iter_mut().find(|(n, _)| *n == name) {
            Some((_, values)) => values.push(v),
            None => groups.push((name, vec![v])),
        }
    }
    for (name, mut values) in groups {
        let v = if values.len() == 1 && !ALWAYS_ARRAY.contains(&name.as_str()) {
            values.remove(0)
        } else {
            Value::Array(values)
        };
        map.insert(name, v);
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(xml: &str) -> Value {
        serde_json::from_str(&convert(xml).unwrap()).unwrap()
    }

    #[test]
    fn maps_attributes_text_and_children() {
        let v = json(
            r#"<?xml version="1.0"?>
            <gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
              <metadata><name>A &amp; B</name><desc/></metadata>
              <trk>
                <name>  spaced  </name>
                <trkseg>
                  <trkpt lat="35.0" lon="139.0"><ele>100</ele></trkpt>
                </trkseg>
              </trk>
            </gpx>"#,
        );
        assert_eq!(
            v,
            serde_json::json!({
                "gpx": {
                    "@version": "1.1",
                    "@xmlns": "http://www.topografix.com/GPX/1/1",
                    "metadata": { "name": "A & B", "desc": "" },
                    "trk": [{
                        "name": "  spaced  ",
                        "trkseg": [{
                            "trkpt": [{ "@lat": "35.0", "@lon": "139.0", "ele": "100" }]
                        }]
                    }]
                }
            })
        );
    }

    #[test]
    fn repeated_children_become_arrays_and_keep_document_order() {
        let out = convert(
            r#"<gpx><trk><name>t</name></trk><extensions><a>1</a><b>2</b><a>3</a></extensions><wpt lat="1" lon="2"/></gpx>"#,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["gpx"]["extensions"]["a"], serde_json::json!(["1", "3"]));
        assert_eq!(v["gpx"]["extensions"]["b"], "2");
        assert_eq!(
            v["gpx"]["wpt"],
            serde_json::json!([{ "@lat": "1", "@lon": "2" }])
        );
        // キーは XML に出てくる順に並ぶ
        let trk = out.find("\"trk\"").unwrap();
        let ext = out.find("\"extensions\"").unwrap();
        let wpt = out.find("\"wpt\"").unwrap();
        assert!(trk < ext && ext < wpt);
    }

    #[test]
    fn keeps_prefixed_names_and_text_beside_attributes() {
        let v = json(
            r#"<gpx xmlns:gpxtpx="urn:x"><link href="https://example.com">site</link><gpxtpx:hr>120</gpxtpx:hr></gpx>"#,
        );
        assert_eq!(
            v["gpx"]["link"],
            serde_json::json!([{ "@href": "https://example.com", "#text": "site" }])
        );
        assert_eq!(v["gpx"]["gpxtpx:hr"], "120");
        assert_eq!(v["gpx"]["@xmlns:gpxtpx"], "urn:x");
    }

    #[test]
    fn does_not_require_point_values() {
        let v = json(r#"<gpx><trk><trkseg><trkpt lat="35"/></trkseg></trk></gpx>"#);
        assert_eq!(v["gpx"]["trk"][0]["trkseg"][0]["trkpt"][0]["@lat"], "35");
    }

    #[test]
    fn rejects_broken_xml() {
        assert!(convert("<gpx><trk></gpx>").is_err());
        assert!(convert("").is_err());
        assert!(convert("<gpx>&nbsp;</gpx>").is_err());
    }

    #[test]
    fn snapshot() {
        insta::assert_snapshot!(convert(include_str!("../tests/fixtures/sample.gpx")).unwrap());
    }
}
