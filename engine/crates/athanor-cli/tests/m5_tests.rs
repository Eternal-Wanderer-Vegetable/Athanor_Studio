//! M5 测试：publish 端到端（需无头浏览器，缺席自动跳过）+ layout_hash 确定性 +
//! publication 记录完整性。

use std::path::PathBuf;

fn corpus(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../corpus")
        .join(name)
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athanor-m5-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn publish_e2e_and_deterministic_layout_hash() {
    // 浏览器缺席时跳过（AZODOC_BROWSER_PATH 可指定）
    if std::env::var_os("AZODOC_BROWSER_PATH").is_none() && azodoc_pdf::find_browser().is_err() {
        eprintln!("跳过：本机未找到 Chromium/Edge");
        return;
    }

    let dir = tmpdir("e2e");
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(
            &corpus("markdown/basic.md"),
            &doc,
            None,
            "zh-CN",
            None,
            false
        ),
        0
    );

    // 第一次出版
    assert_eq!(
        athanor_cli::cmd_publish(
            &doc,
            &athanor_cli::PublishArgs {
                out: Some(&dir.join("first.pdf")),
                browser: None,
            }
        ),
        0
    );
    let pdf1 = std::fs::read(dir.join("first.pdf")).unwrap();
    assert!(pdf1.starts_with(b"%PDF"), "产物必须是 PDF");

    // 容器内检查：产物 + publication 层
    let mut c = azodoc_container::open(std::fs::read(&doc).unwrap())
        .unwrap()
        .0;
    let layer: serde_json::Value =
        serde_json::from_slice(&c.read_entry("publication/publication.json").unwrap()).unwrap();
    let pubs = layer["publications"].as_array().unwrap();
    let pub_id = pubs[0]["id"].as_str().unwrap().to_string();
    let pdf_in = c
        .read_entry(&format!("compatibility/pdf/{pub_id}.pdf"))
        .unwrap();
    assert!(pdf_in.starts_with(b"%PDF"));
    assert_eq!(pdf_in, pdf1, "容器内产物与外部副本应一致");
    assert_eq!(pubs.len(), 1);
    let p1 = pubs[0].clone();
    assert!(p1["id"].as_str().unwrap().starts_with("pub_"));
    assert!(p1["source_revision"].is_string());
    assert!(!p1["renderer"]["fingerprint"].as_str().unwrap().is_empty());
    let layout1 = p1["layout_hash"].as_str().unwrap().to_string();
    let rev1 = p1["source_revision"].clone();
    drop(c);

    // 第二次出版：同输入同渲染器 → layout_hash 必须一致（确定性验收）
    assert_eq!(
        athanor_cli::cmd_publish(
            &doc,
            &athanor_cli::PublishArgs {
                out: None,
                browser: None,
            }
        ),
        0
    );
    let mut c = azodoc_container::open(std::fs::read(&doc).unwrap())
        .unwrap()
        .0;
    let layer2: serde_json::Value =
        serde_json::from_slice(&c.read_entry("publication/publication.json").unwrap()).unwrap();
    let pubs2 = layer2["publications"].as_array().unwrap();
    assert_eq!(pubs2.len(), 2, "出版记录追加不覆盖");
    assert_eq!(pubs2[1]["layout_hash"].as_str(), Some(layout1.as_str()));
    assert_eq!(pubs2[1]["source_revision"], rev1);

    // verify 仍通过（产物 sha 一致性 + 记录完整性）
    assert_eq!(athanor_cli::verify_cmd::run(&doc), 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn publish_tampered_artifact_is_detected() {
    if std::env::var_os("AZODOC_BROWSER_PATH").is_none() && azodoc_pdf::find_browser().is_err() {
        eprintln!("跳过：本机未找到 Chromium/Edge");
        return;
    }
    let dir = tmpdir("tamper");
    let doc = dir.join("d.azodoc");
    assert_eq!(
        athanor_cli::cmd_import(
            &corpus("markdown/basic.md"),
            &doc,
            None,
            "zh-CN",
            None,
            false
        ),
        0
    );
    assert_eq!(
        athanor_cli::cmd_publish(
            &doc,
            &athanor_cli::PublishArgs {
                out: None,
                browser: None
            }
        ),
        0
    );

    // 篡改 PDF 产物一个字节 → verify 必须报错
    let data = std::fs::read(&doc).unwrap();
    let (mut c, _) = azodoc_container::open(data.clone()).unwrap();
    let layer: serde_json::Value =
        serde_json::from_slice(&c.read_entry("publication/publication.json").unwrap()).unwrap();
    let pub_id = layer["publications"][0]["id"].as_str().unwrap().to_string();
    let artifact_path = format!("compatibility/pdf/{pub_id}.pdf");
    let mut pdf = c.read_entry(&artifact_path).unwrap();
    let last = pdf.len() - 1;
    pdf[last] ^= 0xFF;
    c.set_entry(&artifact_path, pdf).unwrap();
    std::fs::write(&doc, c.write().unwrap()).unwrap();

    assert_ne!(athanor_cli::verify_cmd::run(&doc), 0, "篡改产物必须被检出");
    std::fs::remove_dir_all(&dir).ok();
}
