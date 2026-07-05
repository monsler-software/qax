// C ABI shim over Qt6. Every type is exposed as an opaque pointer so the Rust
// side never needs to know Qt's C++ layout. Kept intentionally small and flat:
// one free function per operation, no inheritance leaking across the boundary.
//
// Declarations are always visible; which ones get a *definition* in shim.cpp is
// controlled by the QT6RS_* feature macros build.rs passes in. A declaration
// without a definition is harmless as long as Rust (gated by matching cargo
// features) never references it.
#ifndef QT6_RS_SHIM_H
#define QT6_RS_SHIM_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct QtApp QtApp;
typedef struct QtEngine QtEngine;
typedef struct QtPropertyMap QtPropertyMap;
typedef struct QtObject QtObject;   // erased QObject*
typedef struct QtWidget QtWidget;   // erased QWidget*
typedef struct QtLayout QtLayout;   // erased QLayout*
typedef struct QtPainter QtPainter; // erased QPainter*, live only during a paint callback
typedef struct QtTranslator QtTranslator; // erased QTranslator*

// Signal callback shapes. `user` is an opaque Rust pointer round-tripped back.
typedef void (*QtVoidCb)(void *user);
typedef void (*QtIntCb)(void *user, int value);
typedef void (*QtDoubleCb)(void *user, double value);
typedef void (*QtBoolCb)(void *user, int value);
typedef void (*QtStrCb)(void *user, const char *value);
// Paint callback for custom-drawn widgets: hands back a painter bound to the
// widget plus its current size. The painter is valid only for the call.
typedef void (*QtPaintCb)(void *user, QtPainter *p, int w, int h);

// Discriminant returned by the *_kind accessors, mirrored by qax::VariantKind.
typedef enum {
    QT_VK_INVALID = 0,
    QT_VK_I64 = 1,
    QT_VK_F64 = 2,
    QT_VK_BOOL = 3,
    QT_VK_STRING = 4,
} QtVariantKind;

// ---- Application / event loop ---------------------------------------------
QtApp *qt_app_new(void);
int qt_app_exec(QtApp *app);
void qt_app_quit(QtApp *app);
void qt_app_delete(QtApp *app);

// ---- QML engine (feature: qml) --------------------------------------------
QtEngine *qt_qml_engine_new(void);
void qt_qml_engine_load_file(QtEngine *e, const char *path);
void qt_qml_engine_load_url(QtEngine *e, const char *url);
void qt_qml_engine_load_data(QtEngine *e, const char *data, size_t len,
                             const char *url);
int qt_qml_engine_root_count(QtEngine *e);
void qt_qml_engine_set_context_object(QtEngine *e, const char *name,
                                      QtObject *obj);
void qt_qml_engine_delete(QtEngine *e);

QtPropertyMap *qt_property_map_new(void);
QtObject *qt_property_map_as_object(QtPropertyMap *m);
void qt_property_map_set_i64(QtPropertyMap *m, const char *key, int64_t v);
void qt_property_map_set_f64(QtPropertyMap *m, const char *key, double v);
void qt_property_map_set_bool(QtPropertyMap *m, const char *key, int v);
void qt_property_map_set_str(QtPropertyMap *m, const char *key, const char *v);
QtVariantKind qt_property_map_kind(QtPropertyMap *m, const char *key);
int64_t qt_property_map_get_i64(QtPropertyMap *m, const char *key);
double qt_property_map_get_f64(QtPropertyMap *m, const char *key);
int qt_property_map_get_bool(QtPropertyMap *m, const char *key);
char *qt_property_map_get_str(QtPropertyMap *m, const char *key);
void qt_property_map_on_changed(QtPropertyMap *m, QtStrCb cb, void *user);
void qt_property_map_delete(QtPropertyMap *m);

// ---- Widgets base (feature: widgets) --------------------------------------
QtWidget *qt_widget_new(void);
// Deletes a widget (and, being a parent, its whole child tree + layouts). Used
// by the reactive runtime to tear a window down on drop.
void qt_widget_delete(QtWidget *w);
void qt_widget_show(QtWidget *w);
void qt_widget_set_window_title(QtWidget *w, const char *title);
void qt_widget_resize(QtWidget *w, int width, int height);
void qt_widget_set_layout(QtWidget *w, QtLayout *layout);
void qt_widget_set_enabled(QtWidget *w, int enabled);
void qt_widget_set_fixed_size(QtWidget *w, int width, int height);
// Release a fixed size set earlier, letting the layout size the widget again.
void qt_widget_unset_fixed_size(QtWidget *w);
// Schedule / force a repaint of a widget (custom canvases repaint after diffs).
void qt_widget_update(QtWidget *w);
void qt_widget_repaint(QtWidget *w);

// ---- custom-drawn widget ---------------------------------------------------
// A QWidget whose paintEvent forwards to a Rust callback. This is how the safe
// `CustomWidget` API paints without exposing raw pointers to the user.
QtWidget *qt_canvas_new(QtPaintCb cb, void *user);
// Painter drawing ops (call only from inside the paint callback). Colours are
// 0-255 RGBA. `line` is the pen width in px.
void qt_painter_fill_rect(QtPainter *p, int x, int y, int w, int h, int r, int g,
                          int b, int a);
void qt_painter_stroke_rect(QtPainter *p, int x, int y, int w, int h, int line,
                            int r, int g, int b, int a);
void qt_painter_fill_ellipse(QtPainter *p, int x, int y, int w, int h, int r,
                             int g, int b, int a);
void qt_painter_draw_line(QtPainter *p, int x1, int y1, int x2, int y2, int line,
                          int r, int g, int b, int a);
void qt_painter_draw_text(QtPainter *p, int x, int y, const char *s, int r,
                          int g, int b, int a);
// Suppress signal emission while a value is set programmatically. Returns the
// previous blocked state so the caller can restore it.
int qt_widget_block_signals(QtWidget *w, int block);

// Post a callback to run on the next event-loop iteration (QTimer 0ms). Used to
// defer a reactive re-render out of the signal handler that triggered it, so the
// diff never runs while a widget's own callback is on the stack.
void qt_post(QtVoidCb cb, void *user);

QtWidget *qt_label_new(const char *text);
void qt_label_set_text(QtWidget *label, const char *text);

QtWidget *qt_button_new(const char *text);
void qt_button_set_text(QtWidget *button, const char *text);
void qt_button_on_clicked(QtWidget *button, QtVoidCb cb, void *user);

QtLayout *qt_box_layout_new(int vertical);
void qt_layout_add_widget(QtLayout *layout, QtWidget *child);
void qt_layout_add_layout(QtLayout *layout, QtLayout *child);
void qt_layout_add_stretch(QtLayout *layout);
void qt_layout_set_spacing(QtLayout *layout, int spacing);
void qt_layout_set_margins(QtLayout *layout, int l, int t, int r, int b);
// Positional edits used by the reactive diff to reconcile a container's
// children in place: insert at an index, or remove (and delete) the item there.
void qt_layout_insert_widget(QtLayout *layout, int index, QtWidget *child);
void qt_layout_insert_layout(QtLayout *layout, int index, QtLayout *child);
void qt_layout_insert_stretch(QtLayout *layout, int index);
void qt_layout_remove_at(QtLayout *layout, int index);
// Removes and deletes every item (and its widget) from the layout.
void qt_layout_clear(QtLayout *layout);

// ---- checkbox (feature: checkbox) -----------------------------------------
QtWidget *qt_checkbox_new(const char *text);
void qt_checkbox_set_text(QtWidget *w, const char *text);
void qt_checkbox_set_checked(QtWidget *w, int checked);
int qt_checkbox_is_checked(QtWidget *w);
void qt_checkbox_on_toggled(QtWidget *w, QtBoolCb cb, void *user);

// ---- line edit (feature: line-edit) ---------------------------------------
QtWidget *qt_line_edit_new(const char *text);
void qt_line_edit_set_text(QtWidget *w, const char *text);
char *qt_line_edit_text(QtWidget *w);
void qt_line_edit_set_placeholder(QtWidget *w, const char *text);
void qt_line_edit_on_changed(QtWidget *w, QtStrCb cb, void *user);

// ---- slider (feature: slider) ---------------------------------------------
QtWidget *qt_slider_new(int min, int max, int value);
void qt_slider_set_value(QtWidget *w, int value);
int qt_slider_value(QtWidget *w);
void qt_slider_on_changed(QtWidget *w, QtIntCb cb, void *user);

// ---- spinbox (feature: spinbox) -------------------------------------------
QtWidget *qt_spinbox_new(int min, int max, int value);
void qt_spinbox_set_value(QtWidget *w, int value);
int qt_spinbox_value(QtWidget *w);
void qt_spinbox_on_changed(QtWidget *w, QtIntCb cb, void *user);

// ---- progress bar (feature: progress-bar) ---------------------------------
QtWidget *qt_progress_bar_new(int min, int max, int value);
void qt_progress_bar_set_value(QtWidget *w, int value);

// ---- combo box (feature: combo-box) ---------------------------------------
QtWidget *qt_combo_box_new(void);
void qt_combo_box_add_item(QtWidget *w, const char *text);
void qt_combo_box_clear(QtWidget *w);
int qt_combo_box_current_index(QtWidget *w);
void qt_combo_box_set_current_index(QtWidget *w, int index);
void qt_combo_box_on_changed(QtWidget *w, QtIntCb cb, void *user);

// ---- radio button (feature: radio-button) ---------------------------------
QtWidget *qt_radio_button_new(const char *text);
void qt_radio_button_set_text(QtWidget *w, const char *text);
void qt_radio_button_set_checked(QtWidget *w, int checked);
int qt_radio_button_is_checked(QtWidget *w);
void qt_radio_button_on_toggled(QtWidget *w, QtBoolCb cb, void *user);

// ---- multi-line text edit (feature: text-edit) ----------------------------
QtWidget *qt_text_edit_new(const char *text);
void qt_text_edit_set_text(QtWidget *w, const char *text);
char *qt_text_edit_text(QtWidget *w);
void qt_text_edit_set_placeholder(QtWidget *w, const char *text);
void qt_text_edit_set_read_only(QtWidget *w, int read_only);
void qt_text_edit_on_changed(QtWidget *w, QtStrCb cb, void *user);

// ---- dial (feature: dial) -------------------------------------------------
QtWidget *qt_dial_new(int min, int max, int value);
void qt_dial_set_value(QtWidget *w, int value);
int qt_dial_value(QtWidget *w);
void qt_dial_on_changed(QtWidget *w, QtIntCb cb, void *user);

// ---- double spin box (feature: double-spinbox) ----------------------------
QtWidget *qt_double_spinbox_new(double min, double max, double value,
                                int decimals, double step);
void qt_double_spinbox_set_value(QtWidget *w, double value);
double qt_double_spinbox_value(QtWidget *w);
void qt_double_spinbox_on_changed(QtWidget *w, QtDoubleCb cb, void *user);

// ---- group box (feature: group-box) ---------------------------------------
QtWidget *qt_group_box_new(const char *title);
void qt_group_box_set_title(QtWidget *w, const char *title);

// ---- separator (feature: separator) ---------------------------------------
QtWidget *qt_separator_new(int vertical);

// ---- i18n / resources ------------------------------------------------------
// Look up a translation for (context, source) in the installed translators.
// Returns a malloc'd UTF-8 string the caller must free with qt_string_free.
char *qt_translate(const char *context, const char *source);
// Loads a compiled .qm catalogue from disk and installs it. Returns an opaque
// translator handle (kept installed for the app's lifetime) or NULL on failure.
QtTranslator *qt_translator_load(const char *qm_path);
// Registers an in-memory compiled resource bundle (.rcc produced by `rcc
// --binary`), making its files visible under the `:/` virtual filesystem.
// `data` must outlive the application. Returns non-zero on success.
int qt_resource_register(const unsigned char *data);

// ---- misc ------------------------------------------------------------------
void qt_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif // QT6_RS_SHIM_H
