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
pub enum QtTimer {}
pub enum QtPath {}
pub enum QtImage {}
pub enum QtMenu {}

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
pub type MouseCb =
    extern "C" fn(user: *mut c_void, kind: c_int, x: c_int, y: c_int, button: c_int);
pub type ResizeCb = extern "C" fn(user: *mut c_void, w: c_int, h: c_int);
pub type WheelCb = extern "C" fn(user: *mut c_void, x: c_int, y: c_int, delta: c_int);

unsafe extern "C" {
    // Application / event loop
    pub fn qt_app_new() -> *mut QtApp;
    pub fn qt_app_exec(app: *mut QtApp) -> c_int;
    pub fn qt_app_run_for(app: *mut QtApp, ms: c_int) -> c_int;
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
    pub fn qt_post_to_main(cb: VoidCb, user: *mut c_void);

    // Custom-drawn widget + painter
    pub fn qt_canvas_new(cb: PaintCb, user: *mut c_void) -> *mut QtWidget;
    pub fn qt_canvas_on_mouse(w: *mut QtWidget, cb: MouseCb, user: *mut c_void, track: c_int);
    pub fn qt_canvas_set_mouse_tracking(w: *mut QtWidget, track: c_int);
    pub fn qt_canvas_send_mouse(w: *mut QtWidget, kind: c_int, x: c_int, y: c_int, button: c_int);
    pub fn qt_canvas_on_resize(w: *mut QtWidget, cb: ResizeCb, user: *mut c_void);
    pub fn qt_canvas_on_wheel(w: *mut QtWidget, cb: WheelCb, user: *mut c_void);

    // Painter: state / transforms / quality
    pub fn qt_painter_save(p: *mut QtPainter);
    pub fn qt_painter_restore(p: *mut QtPainter);
    pub fn qt_painter_translate(p: *mut QtPainter, dx: c_double, dy: c_double);
    pub fn qt_painter_rotate(p: *mut QtPainter, degrees: c_double);
    pub fn qt_painter_scale(p: *mut QtPainter, sx: c_double, sy: c_double);
    pub fn qt_painter_set_opacity(p: *mut QtPainter, opacity: c_double);
    pub fn qt_painter_set_antialiasing(p: *mut QtPainter, on: c_int);
    pub fn qt_painter_set_font(p: *mut QtPainter, family: *const c_char, px: c_int, bold: c_int);
    pub fn qt_painter_stroke_ellipse(
        p: *mut QtPainter,
        x: c_int, y: c_int, w: c_int, h: c_int, line: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_fill_rounded_rect(
        p: *mut QtPainter,
        x: c_int, y: c_int, w: c_int, h: c_int, rx: c_double, ry: c_double,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_stroke_rounded_rect(
        p: *mut QtPainter,
        x: c_int, y: c_int, w: c_int, h: c_int, rx: c_double, ry: c_double, line: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_fill_polygon(
        p: *mut QtPainter, pts: *const c_int, n: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_draw_polyline(
        p: *mut QtPainter, pts: *const c_int, n: c_int, line: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_fill_rect_lgrad(
        p: *mut QtPainter,
        x: c_int, y: c_int, w: c_int, h: c_int,
        x1: c_double, y1: c_double, x2: c_double, y2: c_double,
        r1: c_int, g1: c_int, b1: c_int, a1: c_int,
        r2: c_int, g2: c_int, b2: c_int, a2: c_int,
    );
    pub fn qt_painter_fill_rect_rgrad(
        p: *mut QtPainter,
        x: c_int, y: c_int, w: c_int, h: c_int,
        cx: c_double, cy: c_double, radius: c_double,
        r1: c_int, g1: c_int, b1: c_int, a1: c_int,
        r2: c_int, g2: c_int, b2: c_int, a2: c_int,
    );

    // Painter path
    pub fn qt_path_new() -> *mut QtPath;
    pub fn qt_path_move_to(path: *mut QtPath, x: c_double, y: c_double);
    pub fn qt_path_line_to(path: *mut QtPath, x: c_double, y: c_double);
    pub fn qt_path_cubic_to(
        path: *mut QtPath,
        c1x: c_double, c1y: c_double, c2x: c_double, c2y: c_double,
        ex: c_double, ey: c_double,
    );
    pub fn qt_path_close(path: *mut QtPath);
    pub fn qt_path_delete(path: *mut QtPath);
    pub fn qt_painter_fill_path(
        p: *mut QtPainter, path: *mut QtPath, r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_stroke_path(
        p: *mut QtPainter, path: *mut QtPath, line: c_int,
        r: c_int, g: c_int, b: c_int, a: c_int,
    );
    pub fn qt_painter_clip_path(p: *mut QtPainter, path: *mut QtPath);

    // Image
    pub fn qt_image_load(path: *const c_char) -> *mut QtImage;
    pub fn qt_image_from_data(data: *const u8, len: c_int) -> *mut QtImage;
    pub fn qt_image_width(i: *mut QtImage) -> c_int;
    pub fn qt_image_height(i: *mut QtImage) -> c_int;
    pub fn qt_image_delete(i: *mut QtImage);
    pub fn qt_painter_draw_image(p: *mut QtPainter, i: *mut QtImage, x: c_int, y: c_int);
    pub fn qt_painter_draw_image_scaled(
        p: *mut QtPainter, i: *mut QtImage, x: c_int, y: c_int, w: c_int, h: c_int,
    );
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

    // List widget
    pub fn qt_list_new() -> *mut QtWidget;
    pub fn qt_list_add_item(w: *mut QtWidget, text: *const c_char);
    pub fn qt_list_clear(w: *mut QtWidget);
    pub fn qt_list_current_row(w: *mut QtWidget) -> c_int;
    pub fn qt_list_set_current_row(w: *mut QtWidget, row: c_int);
    pub fn qt_list_on_current_changed(w: *mut QtWidget, cb: IntCb, user: *mut c_void);
    pub fn qt_list_on_activated(w: *mut QtWidget, cb: IntCb, user: *mut c_void);

    // Main window + menus
    pub fn qt_main_window_new() -> *mut QtWidget;
    pub fn qt_main_window_set_central(mw: *mut QtWidget, central: *mut QtWidget);
    pub fn qt_main_window_set_status(mw: *mut QtWidget, text: *const c_char);
    pub fn qt_main_window_add_menu(mw: *mut QtWidget, title: *const c_char) -> *mut QtMenu;
    pub fn qt_menu_add_action(menu: *mut QtMenu, text: *const c_char, cb: VoidCb, user: *mut c_void);
    pub fn qt_menu_add_separator(menu: *mut QtMenu);
    pub fn qt_menu_add_submenu(menu: *mut QtMenu, title: *const c_char) -> *mut QtMenu;

    // Dialogs
    pub fn qt_dialog_message(title: *const c_char, text: *const c_char);
    pub fn qt_dialog_confirm(title: *const c_char, text: *const c_char) -> c_int;
    pub fn qt_dialog_input(
        title: *const c_char,
        label: *const c_char,
        initial: *const c_char,
    ) -> *mut c_char;
    pub fn qt_dialog_open_file(
        title: *const c_char,
        dir: *const c_char,
        filter: *const c_char,
    ) -> *mut c_char;
    pub fn qt_dialog_save_file(
        title: *const c_char,
        dir: *const c_char,
        filter: *const c_char,
    ) -> *mut c_char;
    pub fn qt_dialog_open_dir(title: *const c_char, dir: *const c_char) -> *mut c_char;
    pub fn qt_popup_menu(items: *const *const c_char, n: c_int, x: c_int, y: c_int) -> c_int;

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

    // Timer
    pub fn qt_timer_new(interval_ms: c_int, cb: VoidCb, user: *mut c_void) -> *mut QtTimer;
    pub fn qt_timer_set_interval(t: *mut QtTimer, interval_ms: c_int);
    pub fn qt_timer_start(t: *mut QtTimer);
    pub fn qt_timer_stop(t: *mut QtTimer);
    pub fn qt_timer_delete(t: *mut QtTimer);

    // i18n / resources
    pub fn qt_translate(context: *const c_char, source: *const c_char) -> *mut c_char;
    pub fn qt_translator_load(qm_path: *const c_char) -> *mut QtTranslator;
    pub fn qt_resource_register(data: *const u8) -> c_int;

    // misc
    pub fn qt_string_free(s: *mut c_char);
}
