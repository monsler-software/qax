//! # qax — high-level Qt 6 bindings for Rust
//!
//! Ergonomic, safe wrappers over Qt 6 focused on two workflows:
//!
//! 1. **Loading QML** and driving it from Rust.
//! 2. **Composing components from Rust code** and binding them to the UI in a
//!    reactive, idiomatic style.
//!
//! ## Architecture
//!
//! ```text
//!   your app  ──►  qax (safe API)  ──►  qax-sys (raw FFI)  ──►  cpp/shim  ──►  Qt6
//! ```
//!
//! * [`Application`] owns the `QGuiApplication` and its event loop.
//! * [`QmlEngine`] loads QML documents.
//! * [`Model`] is a set of named [`Value`] fields shared with QML. It is backed
//!   by `QQmlPropertyMap`, so QML sees plain, bindable properties while Rust
//!   gets typed [`Model::get`]/[`Model::set`] plus a single
//!   [`Model::on_change`] channel that fires for writes from either side.
//! * [`reactive::Property`] provides Qt-free observable state for building the
//!   logic layer in pure Rust before projecting it into a [`Model`].
//!
//! The whole Qt surface crosses one narrow C ABI (`qax-sys`), so adding a class
//! means adding a handful of flat functions to the shim — no per-type moc code
//! generation is required, which is what keeps the binding maintainable.
//!
//! ## Example
//!
//! ```no_run
//! use qax::{Application, QmlEngine, Model};
//!
//! const QML: &str = r#"
//! import QtQuick
//! import QtQuick.Window
//! Window {
//!     visible: true; width: 320; height: 120
//!     Text { anchors.centerIn: parent; text: backend.greeting }
//!     MouseArea { anchors.fill: parent; onClicked: backend.clicks += 1 }
//! }"#;
//!
//! let app = Application::new();
//! let mut engine = QmlEngine::new();
//!
//! let mut backend = Model::new();
//! backend.set("greeting", "Привет из Rust");
//! backend.set("clicks", 0i64);
//! backend.on_change(|key, fields| {
//!     println!("{key} changed -> {:?}", fields.get(key));
//! });
//!
//! engine.set_context("backend", &backend);
//! engine.load_data(QML, "app.qml");
//! app.exec();
//! ```

mod app;
mod engine;
pub mod i18n;
mod model;
pub mod reactive;
pub mod ui;
mod value;

/// Raw FFI surface (`qax-sys`), re-exported as a low-level escape hatch. The safe
/// API — including custom-drawn [`ui::CustomWidget`]s via [`ui::Canvas`] — should
/// cover normal use; reach for this only to call Qt operations not yet wrapped.
pub use qax_sys as sys;

pub use app::Application;
pub use engine::QmlEngine;
pub use model::{Fields, Model};
pub use reactive::Property;
pub use ui::{Canvas, Color, Component, CustomWidget, Element, Ui};
pub use value::{IntoValue, Value};
