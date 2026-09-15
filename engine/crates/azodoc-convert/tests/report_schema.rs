// This file is part of Athanor, the Azodoc document engine.
// Copyright (C) 2026 Athanor Studio
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published
// by the Free Software Foundation, version 3 of the License only.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! conversion-report 必须符合 spec/json-schema/conversion-report.schema.json。

use azodoc_convert::{build_report, LossClass, LossLog, ReportMeta};

#[test]
fn report_validates_against_spec_schema() {
    let mut log = LossLog::new();
    log.node_none();
    log.node_none();
    log.record(
        LossClass::PreservedRaw,
        "inline_html",
        Some("line 7".into()),
        "preserved",
        "preserved/html/part-0001.html",
        "内联 HTML 以原始形式保留".to_string(),
    );

    let report = build_report(
        &ReportMeta {
            direction: "import",
            source_format: "markdown",
            target_format: "azodoc",
            source_path: Some("input.md"),
            source_sha256: Some(&"a".repeat(64)),
            document_id: Some("doc_01JAVG2Z7Q4M8XK3N5P9R2T6W8"),
            revision: None,
            converter: "athanor-md",
        },
        &log,
    );

    let schema_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../spec/json-schema/conversion-report.schema.json"
    );
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).unwrap()).unwrap();
    jsonschema::validate(&schema, &report)
        .unwrap_or_else(|e| panic!("报告不符合 Schema: {e}\n{report}"));
}

#[test]
fn empty_loss_report_is_complete() {
    let log = LossLog::new();
    let report = build_report(
        &ReportMeta {
            direction: "export",
            source_format: "azodoc",
            target_format: "text",
            source_path: None,
            source_sha256: None,
            document_id: None,
            revision: None,
            converter: "athanor-txt",
        },
        &log,
    );
    assert_eq!(report["status"], "complete");
    assert_eq!(report["summary"]["loss"]["none"], 0);
}
