//! Raw FFI declarations for the Qt6 C shim. This crate is `unsafe` by nature and
//! not meant for direct use — the `qax` crate wraps it into a safe API.
//!
//! Every Qt type crosses the boundary as an opaque pointer; see `cpp/shim.h`.
#![allow(non_camel_case_types)]

use std::os::raw::{c_char, c_double, c_int, c_longlong, c_void};

/// Opaque handles mirroring the C shim's forward-declared structs.
pub enum QtApp {}
pub enum QtEngine {}
pub enum QtPropertyMap {}
pub enum QtObject {}
pub enum QtWidget {}
pub enum QtLayout {}
pub enum QtPainter {}
pub enum QtTranslator {}

/// Mirrors `QtVariantKind` in `shim.h`.
pub const QT_VK_INVALID: c_int = 0;
pub const QT_VK_I64: c_int = 1;
pub const QT_VK_F64: c_int = 2;
pub const QT_VK_BOOL: c_int = 3;
pub const QT_VK_STRING: c_int = 4;

pub type VoidCb = extern "C" fn(user: *mut c_void);
pub type IntCb = extern "C" fn(user: *mut c_void, value: c_int);
pub type DoubleCb = extern "C" fn(user: *mut c_void, value: c_double);
pub type BoolCb = extern "C" fn(user: *mut c_void, value: c_int);
pub type StrCb = extern "C" fn(user: *mut c_void, value: *const c_char);
pub type PaintCb = extern "C" fn(user: *mut c_void, p: *mut QtPainter, w: c_int, h: c_int);

unsafe extern "C" {
    // Application / event loop
    pub fn qt_app_new() -> *mut QtApp;
    pub fn qt_app_exec(app: *mut QtApp) -> c_int;
    pub fn qt_app_quit(app: *mut QtApp);
    pub fn qt_app_delete(app: *mut QtApp);

    // QML engine
    pub fn qt_qml_engine_new() -> *mut QtEngine;
    pub fn qt_qml_engine_load_file(e: *mut QtEngine, path: *const c_char);
    pub fn qt_qml_engine_load_url(e: *mut QtEngine, url: *const c_char);
    pub fn qt_qml_engine_load_data(
        e: *mut QtEngine,
        data: *const c_char,
        len: usize,
        url: *const c_char,
    );
    pub fn qt_qml_engine_root_count(e: *mut QtEngine) -> c_int;
    pub fn qt_qml_engine_set_context_object(
        e: *mut QtEngine,
        name: *const c_char,
        obj: *mut QtObject,
    );
    pub fn qt_qml_engine_delete(e: *mut QtEngine);

    // QQmlPropertyMap
    pub fn qt_property_map_new() -> *mut QtPropertyMap;
    pub fn qt_property_map_as_object(m: *mut QtPropertyMap) -> *mut QtObject;
    pub fn qt_property_map_set_i64(m: *mut QtPropertyMap, key: *const c_char, v: c_longlong);
    pub fn qt_property_map_set_f64(m: *mut QtPropertyMap, key: *const c_char, v: c_double);
    pub fn qt_property_map_set_bool(m: *mut QtPropertyMap, key: *const c_char, v: c_int);
    pub fn qt_property_map_set_str(m: *mut QtPropertyMap, key: *const c_char, v: *const c_char);
    pub fn qt_property_map_kind(m: *mut QtPropertyMap, key: *const c_char) -> c_int;
    pub fn qt_property_map_get_i64(m: *mut QtPropertyMap, key: *const c_char) -> c_longlong;
    pub fn qt_property_map_get_f64(m: *mut QtPropertyMap, key: *const c_char) -> c_double;
    pub fn qt_property_map_get_bool(m: *mut QtPropertyMap, key: *const c_char) -> c_int;
    pub fn qt_property_map_get_str(m: *mut QtPropertyMap, key: *const c_char) -> *mut c_char;
    pub fn qt_property_map_on_changed(m: *mut QtPropertyMap, cb: StrCb, user: *mut c_void);
    pub fn qt_property_map_delete(m: *mut QtPropertyMap);

    // Widgets
    pub fn qt_widget_new() -> *mut QtWidget;
    pub fn qt_widget_delete(w: *mut QtWidget);
    pub fn qt_widget_show(w: *mut QtWidget);
    pub fn qt_widget_set_window_title(w: *mut QtWidget, title: *const c_char);
    pub fn qt_widget_resize(w: *mut QtWidget, width: c_int, height: c_int);
    pub fn qt_widget_set_layout(w: *mut QtWidget, layout: *mut QtLayout);
    pub fn qt_widget_set_enabled(w: *mut QtWidget, enabled: c_int);
    pub fn qt_widget_block_signals(w: *mut QtWidget, block: c_int) -> c_int;
    pub fn qt_widget_set_fixed_size(w: *mut QtWidget, width: c_int, height: c_int);
    pub fn qt_widget_unset_fixed_size(w: *mut QtWidget);
    pub fn qt_widget_update(w: *mut QtWidget);
    pub fn qt_widget_repaint(w: *mut QtWidget);
    pub fn qt_post(cb: VoidCb, user: *mut c_void);

    // Custom-drawn widget + painter
    pub fn qt_canvas_new(cb: PaintCb, user: *mut c_void) -> *mut QtWidget;
    pub fn qt_painter_fill_rect(
        p: *mut QtPainter,
        x: c_int, y: c_int, w: c_int, h: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_stroke_rect(
        p: *mut QtPainter,
        x: c_int, y: c_int, w: c_int, h: c_int, line: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_fill_ellipse(
        p: *mut QtPainter,
        x: c_int, y: c_int, w: c_int, h: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_draw_line(
        p: *mut QtPainter,
        x1: c_int, y1: c_int, x2: c_int, y2: c_int, line: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_draw_text(
        p: *mut QtPainter,
        x: c_int, y: c_int, s: *const c_char,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_label_new(text: *const c_char) -> *mut QtWidget;
    pub fn qt_label_set_text(label: *mut QtWidget, text: *const c_char);
    pub fn qt_button_new(text: *const c_char) -> *mut QtWidget;
    pub fn qt_button_set_text(button: *mut QtWidget, text: *const c_char);
    pub fn qt_button_on_clicked(button: *mut QtWidget, cb: VoidCb, user: *mut c_void);
    pub fn qt_box_layout_new(vertical: c_int) -> *mut QtLayout;
    pub fn qt_layout_add_widget(layout: *mut QtLayout, child: *mut QtWidget);
    pub fn qt_layout_add_layout(layout: *mut QtLayout, child: *mut QtLayout);
    pub fn qt_layout_add_stretch(layout: *mut QtLayout);
    pub fn qt_layout_set_spacing(layout: *mut QtLayout, spacing: c_int);
    pub fn qt_layout_set_margins(layout: *mut QtLayout, l: c_int, t: c_int, r: c_int, b: c_int);
    pub fn qt_layout_insert_widget(layout: *mut QtLayout, index: c_int, child: *mut QtWidget);
    pub fn qt_layout_insert_layout(layout: *mut QtLayout, index: c_int, child: *mut QtLayout);
    pub fn qt_layout_insert_stretch(layout: *mut QtLayout, index: c_int);
    pub fn qt_layout_remove_at(layout: *mut QtLayout, index: c_int);
    pub fn qt_layout_clear(layout: *mut QtLayout);

    // Checkbox
    pub fn qt_checkbox_new(text: *const c_char) -> *mut QtWidget;
    pub fn qt_checkbox_set_text(w: *mut QtWidget, text: *const c_char);
    pub fn qt_checkbox_set_checked(w: *mut QtWidget, checked: c_int);
    pub fn qt_checkbox_is_checked(w: *mut QtWidget) -> c_int;
    pub fn qt_checkbox_on_toggled(w: *mut QtWidget, cb: BoolCb, user: *mut c_void);

    // Line edit
    pub fn qt_line_edit_new(text: *const c_char) -> *mut QtWidget;
    pub fn qt_line_edit_set_text(w: *mut QtWidget, text: *const c_char);
    pub fn qt_line_edit_text(w: *mut QtWidget) -> *mut c_char;
    pub fn qt_line_edit_set_placeholder(w: *mut QtWidget, text: *const c_char);
    pub fn qt_line_edit_on_changed(w: *mut QtWidget, cb: StrCb, user: *mut c_void);

    // Slider
    pub fn qt_slider_new(min: c_int, max: c_int, value: c_int) -> *mut QtWidget;
    pub fn qt_slider_set_value(w: *mut QtWidget, value: c_int);
    pub fn qt_slider_value(w: *mut QtWidget) -> c_int;
    pub fn qt_slider_on_changed(w: *mut QtWidget, cb: IntCb, user: *mut c_void);

    // Spinbox
    pub fn qt_spinbox_new(min: c_int, max: c_int, value: c_int) -> *mut QtWidget;
    pub fn qt_spinbox_set_value(w: *mut QtWidget, value: c_int);
    pub fn qt_spinbox_value(w: *mut QtWidget) -> c_int;
    pub fn qt_spinbox_on_changed(w: *mut QtWidget, cb: IntCb, user: *mut c_void);

    // Progress bar
    pub fn qt_progress_bar_new(min: c_int, max: c_int, value: c_int) -> *mut QtWidget;
    pub fn qt_progress_bar_set_value(w: *mut QtWidget, value: c_int);

    // Combo box
    pub fn qt_combo_box_new() -> *mut QtWidget;
    pub fn qt_combo_box_add_item(w: *mut QtWidget, text: *const c_char);
    pub fn qt_combo_box_clear(w: *mut QtWidget);
    pub fn qt_combo_box_current_index(w: *mut QtWidget) -> c_int;
    pub fn qt_combo_box_set_current_index(w: *mut QtWidget, index: c_int);
    pub fn qt_combo_box_on_changed(w: *mut QtWidget, cb: IntCb, user: *mut c_void);

    // Radio button
    pub fn qt_radio_button_new(text: *const c_char) -> *mut QtWidget;
    pub fn qt_radio_button_set_text(w: *mut QtWidget, text: *const c_char);
    pub fn qt_radio_button_set_checked(w: *mut QtWidget, checked: c_int);
    pub fn qt_radio_button_is_checked(w: *mut QtWidget) -> c_int;
    pub fn qt_radio_button_on_toggled(w: *mut QtWidget, cb: BoolCb, user: *mut c_void);

    // Multi-line text edit
    pub fn qt_text_edit_new(text: *const c_char) -> *mut QtWidget;
    pub fn qt_text_edit_set_text(w: *mut QtWidget, text: *const c_char);
    pub fn qt_text_edit_text(w: *mut QtWidget) -> *mut c_char;
    pub fn qt_text_edit_set_placeholder(w: *mut QtWidget, text: *const c_char);
    pub fn qt_text_edit_set_read_only(w: *mut QtWidget, read_only: c_int);
    pub fn qt_text_edit_on_changed(w: *mut QtWidget, cb: StrCb, user: *mut c_void);

    // Dial
    pub fn qt_dial_new(min: c_int, max: c_int, value: c_int) -> *mut QtWidget;
    pub fn qt_dial_set_value(w: *mut QtWidget, value: c_int);
    pub fn qt_dial_value(w: *mut QtWidget) -> c_int;
    pub fn qt_dial_on_changed(w: *mut QtWidget, cb: IntCb, user: *mut c_void);

    // Double spin box
    pub fn qt_double_spinbox_new(
        min: c_double,
        max: c_double,
        value: c_double,
        decimals: c_int,
        step: c_double,
    ) -> *mut QtWidget;
    pub fn qt_double_spinbox_set_value(w: *mut QtWidget, value: c_double);
    pub fn qt_double_spinbox_value(w: *mut QtWidget) -> c_double;
    pub fn qt_double_spinbox_on_changed(w: *mut QtWidget, cb: DoubleCb, user: *mut c_void);

    // Group box
    pub fn qt_group_box_new(title: *const c_char) -> *mut QtWidget;
    pub fn qt_group_box_set_title(w: *mut QtWidget, title: *const c_char);

    // Separator
    pub fn qt_separator_new(vertical: c_int) -> *mut QtWidget;

    // i18n / resources
    pub fn qt_translate(context: *const c_char, source: *const c_char) -> *mut c_char;
    pub fn qt_translator_load(qm_path: *const c_char) -> *mut QtTranslator;
    pub fn qt_resource_register(data: *const u8) -> c_int;

    // misc
    pub fn qt_string_free(s: *mut c_char);
}
