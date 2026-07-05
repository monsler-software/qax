//! End-to-end demo: a QML window bound to a Rust `Model`.
//!
//! Run with a display available:  `cargo run -p qax --example counter`
//!
//! The button writes `clicks` from QML; Rust observes the change, derives a
//! `label` field, and writes it back — QML re-renders via its binding. This is
//! the full round-trip in ~30 lines.
use std::cell::RefCell;
use std::rc::Rc;

use qax::{Application, Model, QmlEngine};

const QML: &str = r#"
import QtQuick
import QtQuick.Window
import QtQuick.Controls

Window {
    visible: true
    width: 360
    height: 160
    title: "qax"

    Column {
        anchors.centerIn: parent
        spacing: 12
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            font.pixelSize: 20
            text: backend.label
        }
        Button {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "Click me"
            onClicked: backend.clicks += 1
        }
    }
}
"#;

fn main() {
    let app = Application::new();
    let mut engine = QmlEngine::new();

    // Shared model, wrapped so the change callback can also write back into it.
    let backend = Rc::new(RefCell::new(Model::new()));
    {
        let mut b = backend.borrow_mut();
        b.set("clicks", 0i64);
        b.set("label", "No clicks yet");
    }

    // React to QML-driven changes and project a derived field back to the UI.
    {
        let backend_cb = backend.clone();
        backend
            .borrow_mut()
            .on_change(move |key, fields| {
                if key != "clicks" {
                    return;
                }
                let clicks = fields.get("clicks").and_then(|v| v.as_int()).unwrap_or(0);
                // Re-entrant set is fine: it only fires for the "label" key.
                backend_cb
                    .borrow_mut()
                    .set("label", format!("Clicked {clicks} time(s)"));
            });
    }

    engine.set_context("backend", &backend.borrow());
    engine.load_data(QML, "counter.qml");

    if engine.root_count() == 0 {
        eprintln!("QML failed to load");
        return;
    }
    std::process::exit(app.exec());
}
