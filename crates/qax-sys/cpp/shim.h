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
typedef struct QtTimer QtTimer;           // erased QTimer*
typedef struct QtPath QtPath;             // erased QPainterPath*
typedef struct QtImage QtImage;           // erased QImage*
typedef struct QtMenu QtMenu;             // erased QMenu*
typedef struct QtLocale QtLocale;         // erased QLocale* (heap-allocated)

// Signal callback shapes. `user` is an opaque Rust pointer round-tripped back.
typedef void (*QtVoidCb)(void *user);
typedef void (*QtIntCb)(void *user, int value);
typedef void (*QtDoubleCb)(void *user, double value);
typedef void (*QtBoolCb)(void *user, int value);
typedef void (*QtStrCb)(void *user, const char *value);
// Paint callback for custom-drawn widgets: hands back a painter bound to the
// widget plus its current size. The painter is valid only for the call.
typedef void (*QtPaintCb)(void *user, QtPainter *p, int w, int h);
// Mouse callback for custom-drawn widgets. `kind`: 0 = press, 1 = move,
// 2 = release. `x`/`y` are widget-local pixels. `button` is the Qt button code
// that changed (press/release) or the bitmask of buttons held (move).
typedef void (*QtMouseCb)(void *user, int kind, int x, int y, int button);
// Resize callback for custom-drawn widgets: the new widget size in pixels.
typedef void (*QtResizeCb)(void *user, int w, int h);
// Wheel callback for custom-drawn widgets: local position and vertical delta
// (Qt's angleDelta().y(); one notch is typically ±120).
typedef void (*QtWheelCb)(void *user, int x, int y, int delta);

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
// Run the event loop for `ms` milliseconds, then return. Handy for tests/demos.
int qt_app_run_for(QtApp *app, int ms);
void qt_app_quit(QtApp *app);
void qt_app_delete(QtApp *app);
// Application identity (Wayland app id derives from the desktop file name).
void qt_app_set_application_name(const char *name);
void qt_app_set_application_display_name(const char *name);
void qt_app_set_application_version(const char *version);
void qt_app_set_organization_name(const char *name);
void qt_app_set_organization_domain(const char *domain);
void qt_app_set_desktop_file_name(const char *name);

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
// Pin one dimension only (declarative `.width()` / `.height()`); the unset
// variants release just that dimension back to the layout.
void qt_widget_set_fixed_width(QtWidget *w, int width);
void qt_widget_set_fixed_height(QtWidget *w, int height);
void qt_widget_unset_fixed_width(QtWidget *w);
void qt_widget_unset_fixed_height(QtWidget *w);
// Schedule / force a repaint of a widget (custom canvases repaint after diffs).
void qt_widget_update(QtWidget *w);
void qt_widget_repaint(QtWidget *w);
// Apply a Qt Style Sheet (a CSS-like dialect) to a widget; it cascades to the
// widget's children. Pass an empty string to clear any sheet set earlier.
void qt_widget_set_stylesheet(QtWidget *w, const char *css);
// Set (or clear, with an empty string) a widget's hover tooltip text.
void qt_widget_set_tooltip(QtWidget *w, const char *text);
// Show or hide a widget within its layout (hidden widgets take no space).
void qt_widget_set_visible(QtWidget *w, int visible);
// ---- window controls (act on a top-level widget / QMainWindow) -------------
void qt_widget_move(QtWidget *w, int x, int y);
void qt_widget_set_minimum_size(QtWidget *w, int width, int height);
void qt_widget_set_maximum_size(QtWidget *w, int width, int height);
void qt_widget_show_normal(QtWidget *w);
void qt_widget_show_maximized(QtWidget *w);
void qt_widget_show_minimized(QtWidget *w);
void qt_widget_show_fullscreen(QtWidget *w);
void qt_widget_hide(QtWidget *w);
int qt_widget_close(QtWidget *w);
void qt_widget_center(QtWidget *w);
void qt_widget_set_always_on_top(QtWidget *w, int on);
// Installs a callback fired when the widget receives a close event (the user
// clicks the window's close button, or qt_widget_close is called). It only
// observes — the close is never vetoed. The callback must not delete the widget
// synchronously; defer any teardown to a later event-loop turn.
void qt_widget_on_close(QtWidget *w, QtVoidCb cb, void *user);
// Sets an icon built from a (kind, name, fallback) triple: kind 0 loads a file
// or Qt resource path (":/…") given in `name`; kind 1 looks `name` up in the
// active desktop icon theme, falling back to the `fallback` path if absent.
void qt_widget_set_window_icon(QtWidget *w, int kind, const char *name,
                               const char *fallback);

// ---- custom-drawn widget ---------------------------------------------------
// A QWidget whose paintEvent forwards to a Rust callback. This is how the safe
// `CustomWidget` API paints without exposing raw pointers to the user.
QtWidget *qt_canvas_new(QtPaintCb cb, void *user);
// Attach (or replace) a canvas's mouse callback. `track` != 0 enables hover
// tracking so move events fire without a button held; otherwise moves fire only
// during a drag. Press/release always fire regardless of `track`.
void qt_canvas_on_mouse(QtWidget *w, QtMouseCb cb, void *user, int track);
void qt_canvas_set_mouse_tracking(QtWidget *w, int track);
// Synthesize and deliver a mouse event to a canvas (kind/button as in QtMouseCb).
// For tests and input automation.
void qt_canvas_send_mouse(QtWidget *w, int kind, int x, int y, int button);
void qt_canvas_on_resize(QtWidget *w, QtResizeCb cb, void *user);
void qt_canvas_on_wheel(QtWidget *w, QtWheelCb cb, void *user);

// Painter state / transforms / quality (call only from inside a paint callback).
void qt_painter_save(QtPainter *p);
void qt_painter_restore(QtPainter *p);
void qt_painter_translate(QtPainter *p, double dx, double dy);
void qt_painter_rotate(QtPainter *p, double degrees);
void qt_painter_scale(QtPainter *p, double sx, double sy);
void qt_painter_set_opacity(QtPainter *p, double opacity);
void qt_painter_set_antialiasing(QtPainter *p, int on);
void qt_painter_set_font(QtPainter *p, const char *family, int px, int bold);
// Extra shapes.
void qt_painter_stroke_ellipse(QtPainter *p, int x, int y, int w, int h,
                               int line, int r, int g, int b, int a);
void qt_painter_fill_rounded_rect(QtPainter *p, int x, int y, int w, int h,
                                  double rx, double ry, int r, int g, int b,
                                  int a);
void qt_painter_stroke_rounded_rect(QtPainter *p, int x, int y, int w, int h,
                                    double rx, double ry, int line, int r, int g,
                                    int b, int a);
// Polygons: `pts` is 2*n interleaved x,y ints.
void qt_painter_fill_polygon(QtPainter *p, const int *pts, int n, int r, int g,
                             int b, int a);
void qt_painter_draw_polyline(QtPainter *p, const int *pts, int n, int line,
                              int r, int g, int b, int a);
// Two-stop gradient fills of a rectangle.
void qt_painter_fill_rect_lgrad(QtPainter *p, int x, int y, int w, int h,
                                double x1, double y1, double x2, double y2,
                                int r1, int g1, int b1, int a1, int r2, int g2,
                                int b2, int a2);
void qt_painter_fill_rect_rgrad(QtPainter *p, int x, int y, int w, int h,
                                double cx, double cy, double radius, int r1,
                                int g1, int b1, int a1, int r2, int g2, int b2,
                                int a2);
// Painter path: build with the qt_path_* ops, then fill/stroke/clip with it.
QtPath *qt_path_new(void);
void qt_path_move_to(QtPath *path, double x, double y);
void qt_path_line_to(QtPath *path, double x, double y);
void qt_path_cubic_to(QtPath *path, double c1x, double c1y, double c2x,
                      double c2y, double ex, double ey);
void qt_path_close(QtPath *path);
void qt_path_delete(QtPath *path);
void qt_painter_fill_path(QtPainter *p, QtPath *path, int r, int g, int b, int a);
void qt_painter_stroke_path(QtPainter *p, QtPath *path, int line, int r, int g,
                            int b, int a);
void qt_painter_clip_path(QtPainter *p, QtPath *path);
// Images. Load once (holds pixels), draw many times; delete when done.
QtImage *qt_image_load(const char *path);
QtImage *qt_image_from_data(const unsigned char *data, int len);
int qt_image_width(QtImage *i);
int qt_image_height(QtImage *i);
void qt_image_delete(QtImage *i);
void qt_painter_draw_image(QtPainter *p, QtImage *i, int x, int y);
void qt_painter_draw_image_scaled(QtPainter *p, QtImage *i, int x, int y, int w,
                                  int h);
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
// Thread-safe variant of qt_post: may be called from any thread. Marshals `cb`
// onto the GUI thread via a queued connection. Used by the async Emitter.
void qt_post_to_main(QtVoidCb cb, void *user);

QtWidget *qt_label_new(const char *text);
void qt_label_set_text(QtWidget *label, const char *text);

QtWidget *qt_button_new(const char *text);
void qt_button_set_text(QtWidget *button, const char *text);
void qt_button_on_clicked(QtWidget *button, QtVoidCb cb, void *user);
// Full QPushButton surface. `checkable` turns the button into a toggle; a
// checkable button keeps a checked state and emits `toggled` when it flips.
// `flat` draws it without a frame until hovered/pressed; `default` marks it as
// the dialog's default action (activated by Enter).
void qt_button_set_checkable(QtWidget *button, int checkable);
void qt_button_set_checked(QtWidget *button, int checked);
int qt_button_is_checked(QtWidget *button);
void qt_button_set_flat(QtWidget *button, int flat);
void qt_button_set_default(QtWidget *button, int is_default);
// Sets the icon on any QAbstractButton (push button, checkbox, radio button)
// from a (kind, name, fallback) triple; see qt_widget_set_window_icon. An empty
// name clears the icon.
void qt_abstract_button_set_icon(QtWidget *button, int kind, const char *name,
                                 const char *fallback);
void qt_button_on_toggled(QtWidget *button, QtBoolCb cb, void *user);

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
// Grid layout: children are placed at an explicit (row, col) with optional spans.
QtLayout *qt_grid_layout_new();
void qt_grid_layout_add_widget(QtLayout *layout, QtWidget *child, int row, int col,
                               int row_span, int col_span);
void qt_grid_layout_add_layout(QtLayout *layout, QtLayout *child, int row, int col,
                               int row_span, int col_span);

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
// Appends an item carrying an optional icon (see qt_widget_set_window_icon for
// the kind/name/fallback triple; an empty name means no icon).
void qt_combo_box_add_item(QtWidget *w, int kind, const char *name,
                           const char *fallback, const char *text);
void qt_combo_box_clear(QtWidget *w);
int qt_combo_box_current_index(QtWidget *w);
void qt_combo_box_set_current_index(QtWidget *w, int index);
void qt_combo_box_on_changed(QtWidget *w, QtIntCb cb, void *user);

// ---- list widget (feature: list) ------------------------------------------
QtWidget *qt_list_new(void);
void qt_list_add_item(QtWidget *w, int kind, const char *name,
                      const char *fallback, const char *text);
void qt_list_clear(QtWidget *w);
int qt_list_current_row(QtWidget *w);
void qt_list_set_current_row(QtWidget *w, int row);
void qt_list_on_current_changed(QtWidget *w, QtIntCb cb, void *user);
void qt_list_on_activated(QtWidget *w, QtIntCb cb, void *user);

// ---- main window + menus (feature: menu) -----------------------------------
QtWidget *qt_main_window_new(void);
void qt_main_window_set_central(QtWidget *mw, QtWidget *central);
void qt_main_window_set_status(QtWidget *mw, const char *text);
QtMenu *qt_main_window_add_menu(QtWidget *mw, const char *title);
void qt_menu_add_action(QtMenu *menu, const char *text, QtVoidCb cb, void *user);
// Like qt_menu_add_action but with a leading icon (kind/name/fallback triple).
void qt_menu_add_action_icon(QtMenu *menu, int kind, const char *name,
                             const char *fallback, const char *text,
                             QtVoidCb cb, void *user);
void qt_menu_add_separator(QtMenu *menu);
QtMenu *qt_menu_add_submenu(QtMenu *menu, const char *title);

// ---- dialogs (feature: dialog) ---------------------------------------------
void qt_dialog_message(const char *title, const char *text);
int qt_dialog_confirm(const char *title, const char *text);
char *qt_dialog_input(const char *title, const char *label, const char *initial);
char *qt_dialog_open_file(const char *title, const char *dir, const char *filter);
char *qt_dialog_save_file(const char *title, const char *dir, const char *filter);
char *qt_dialog_open_dir(const char *title, const char *dir);
int qt_popup_menu(const char *const *items, int n, int x, int y);

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

// ---- timer (feature: timer) -----------------------------------------------
// A repeating QTimer that fires `cb` every `interval_ms` on the event loop.
// Starts immediately; the caller owns it and must qt_timer_delete it. `user`
// must outlive the timer (or be detached with qt_timer_stop/delete first).
QtTimer *qt_timer_new(int interval_ms, QtVoidCb cb, void *user);
void qt_timer_set_interval(QtTimer *t, int interval_ms);
void qt_timer_start(QtTimer *t);
void qt_timer_stop(QtTimer *t);
void qt_timer_delete(QtTimer *t);

// ---- i18n / resources ------------------------------------------------------
// Look up a translation for (context, source) in the installed translators.
// Returns a malloc'd UTF-8 string the caller must free with qt_string_free.
char *qt_translate(const char *context, const char *source);
// Loads a compiled .qm catalogue from disk and installs it. Returns an opaque
// translator handle (kept installed for the app's lifetime) or NULL on failure.
QtTranslator *qt_translator_load(const char *qm_path);
// Load a catalogue whose base name is `basename` (e.g. "app") suffixed by the
// system UI language (":/i18n/app_ru.qm") from `directory` (a path or ":/…"
// resource dir). Picks the best match for the current locale. NULL on failure.
QtTranslator *qt_translator_load_for_locale(const char *basename,
                                            const char *directory);
// Registers an in-memory compiled resource bundle (.rcc produced by `rcc
// --binary`), making its files visible under the `:/` virtual filesystem.
// `data` must outlive the application. Returns non-zero on success.
int qt_resource_register(const unsigned char *data);

// ---- locale (feature: i18n) ------------------------------------------------
// All accessors returning char* yield a malloc'd UTF-8 string the caller frees
// with qt_string_free. Locale handles are heap-allocated; free with
// qt_locale_delete.
QtLocale *qt_locale_system(void);      // QLocale::system()
QtLocale *qt_locale_c(void);           // the C locale
QtLocale *qt_locale_from_name(const char *name); // e.g. "ru_RU", "en"
QtLocale *qt_locale_clone(QtLocale *l);
void qt_locale_delete(QtLocale *l);
char *qt_locale_name(QtLocale *l);          // "ru_RU"
char *qt_locale_bcp47_name(QtLocale *l);    // "ru-RU"
char *qt_locale_language_name(QtLocale *l); // English name, "Russian"
char *qt_locale_native_language_name(QtLocale *l); // "русский"
char *qt_locale_territory_name(QtLocale *l);       // English name, "Russia"
char *qt_locale_native_territory_name(QtLocale *l);// "Россия"
char *qt_locale_decimal_point(QtLocale *l);
char *qt_locale_group_separator(QtLocale *l);
int qt_locale_is_rtl(QtLocale *l); // 1 if text direction is right-to-left
char *qt_locale_format_i64(QtLocale *l, int64_t v);
// `precision` < 0 uses Qt's default; `fmt` is 'f', 'e', or 'g'.
char *qt_locale_format_f64(QtLocale *l, double v, char fmt, int precision);
// Sets the process-wide default locale used by widgets and QString::toX.
void qt_locale_set_default(QtLocale *l);

// ---- misc ------------------------------------------------------------------
void qt_string_free(char *s);

#ifdef __cplusplus
}
#endif

#endif // QT6_RS_SHIM_H
