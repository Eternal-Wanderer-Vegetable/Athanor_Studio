//! M2 验收 ①（HTML）：`html → azodoc → html → azodoc` 模型层恒等。

use azodoc_convert::{canonicalize_assets, ImportJob};

fn import_html(text: &str) -> (azodoc_convert::ImportOutput, azodoc_convert::ImportJob) {
    let mut job = ImportJob::new(None);
    let out = azodoc_html::import(text, &mut job);
    (out, job)
}

fn asset_ids(content: &serde_json::Value) -> Vec<String> {
    fn collect(v: &serde_json::Value, ids: &mut Vec<String>) {
        if let Some(s) = v.as_str() {
            if let Some(rest) = s.strip_prefix("asset://") {
                let id = rest.split('/').next().unwrap_or(rest);
                if !ids.iter().any(|x| x == id) {
                    ids.push(id.to_string());
                }
            }
            return;
        }
        if let Some(obj) = v.as_object() {
            for val in obj.values() {
                collect(val, ids);
            }
        } else if let Some(arr) = v.as_array() {
            for item in arr {
                collect(item, ids);
            }
        }
    }
    let mut ids = Vec::new();
    collect(content, &mut ids);
    ids
}

#[test]
fn corpus_basic_roundtrip_identity() {
    let html = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../corpus/html/basic.html"
    ))
    .unwrap();

    let (first, job) = import_html(&html);
    let ids = asset_ids(&first.content);
    let doc = azodoc_convert::ExportDoc {
        content: first.content.clone(),
        assets: ids
            .iter()
            .map(|id| azodoc_convert::ExportAsset {
                id: id.clone(),
                filename: "a.png".into(),
                mime: "image/png".into(),
                storage: "external".into(),
                url: Some("https://example.com/a.png".into()),
                bytes: None,
            })
            .collect(),
        preserved: job
            .preserved
            .iter()
            .map(|(p, b)| (p.clone(), b.clone()))
            .collect(),
        title: first.title.clone(),
        language: None,
        document_id: None,
    };
    let mut export_log = azodoc_convert::LossLog::new();
    let html2 = azodoc_html::export_html(&doc, &mut export_log);

    let (second, _) = import_html(&html2);

    let mut a = first.content.clone();
    let mut b = second.content.clone();
    let ia = asset_ids(&a);
    let ib = asset_ids(&b);
    canonicalize_assets(&mut a, &ia);
    canonicalize_assets(&mut b, &ib);
    assert_eq!(
        a, b,
        "HTML 模型层往返恒等失败。\n--- 二次导出的 HTML ---\n{html2}"
    );
}
