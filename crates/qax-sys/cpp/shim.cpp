#include "shim.h"

#include <QApplication>
#include <QBoxLayout>
#include <QCoreApplication>
#include <QByteArray>
#include <QCheckBox>
#include <QComboBox>
#include <QDial>
#include <QDoubleSpinBox>
#include <QFont>
#include <QFrame>
#include <QGroupBox>
#include <QGuiApplication>
#include <QImage>
#include <QFileDialog>
#include <QInputDialog>
#include <QLabel>
#include <QLinearGradient>
#include <QLineEdit>
#include <QListWidget>
#include <QMainWindow>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QMouseEvent>
#include <QPainterPath>
#include <QPlainTextEdit>
#include <QPolygon>
#include <QRadialGradient>
#include <QRadioButton>
#include <QResizeEvent>
#include <QStatusBar>
#include <QWheelEvent>
#include <QColor>
#include <QObject>
#include <QPainter>
#include <QPaintEvent>
#include <QProgressBar>
#include <QPushButton>
#include <QQmlApplicationEngine>
#include <QResource>
#include <QQmlContext>
#include <QQmlPropertyMap>
#include <QSlider>
#include <QSpinBox>
#include <QString>
#include <QTimer>
#include <QTranslator>
#include <QUrl>
#include <QVariant>

#include <cstdlib>
#include <cstring>

namespace {

// QGuiApplication requires argc/argv to outlive it, so back them with statics.
int g_argc = 1;
char g_arg0[] = "qax";
char *g_argv[] = {g_arg0, nullptr};

QString from_c(const char *s) { return s ? QString::fromUtf8(s) : QString(); }

// Duplicate a QString into a malloc'd C string the caller must free.
char *dup_c(const QString &s) {
    QByteArray utf8 = s.toUtf8();
    char *out = static_cast<char *>(std::malloc(utf8.size() + 1));
    std::memcpy(out, utf8.constData(), utf8.size());
    out[utf8.size()] = '\0';
    return out;
}

QWidget *W(QtWidget *w) { return reinterpret_cast<QWidget *>(w); }
QLayout *L(QtLayout *l) { return reinterpret_cast<QLayout *>(l); }
QPainter *P(QtPainter *p) { return reinterpret_cast<QPainter *>(p); }

// A QWidget that forwards its paintEvent to a Rust callback. paintEvent is a
// plain virtual override, so no moc is involved.
class RsCanvas : public QWidget {
public:
    RsCanvas(QtPaintCb cb, void *user) : m_cb(cb), m_user(user) {}

    // Wire up (or replace) the mouse callback and toggle hover tracking. With
    // tracking off, move events fire only while a button is held (dragging).
    void setMouse(QtMouseCb cb, void *user, int track) {
        m_mouse_cb = cb;
        m_mouse_user = user;
        setMouseTracking(track != 0);
    }
    void setMouseTrack(int track) { setMouseTracking(track != 0); }
    void setResize(QtResizeCb cb, void *user) {
        m_resize_cb = cb;
        m_resize_user = user;
    }
    void setWheel(QtWheelCb cb, void *user) {
        m_wheel_cb = cb;
        m_wheel_user = user;
    }

protected:
    void paintEvent(QPaintEvent *) override {
        if (!m_cb)
            return;
        QPainter painter(this);
        m_cb(m_user, reinterpret_cast<QtPainter *>(&painter), width(), height());
    }

    // kind: 0 = press, 1 = move, 2 = release. For press/release we report the
    // single button that changed; for moves, the bitmask of buttons held.
    void mousePressEvent(QMouseEvent *e) override { emitMouse(0, e); }
    void mouseMoveEvent(QMouseEvent *e) override { emitMouse(1, e); }
    void mouseReleaseEvent(QMouseEvent *e) override { emitMouse(2, e); }

    void resizeEvent(QResizeEvent *) override {
        if (m_resize_cb)
            m_resize_cb(m_resize_user, width(), height());
    }
    void wheelEvent(QWheelEvent *e) override {
        if (!m_wheel_cb)
            return;
        const QPointF p = e->position();
        m_wheel_cb(m_wheel_user, static_cast<int>(p.x()),
                   static_cast<int>(p.y()), e->angleDelta().y());
    }

private:
    void emitMouse(int kind, QMouseEvent *e) {
        if (!m_mouse_cb)
            return;
        int btn = kind == 1 ? static_cast<int>(e->buttons())
                            : static_cast<int>(e->button());
        const QPointF p = e->position();
        m_mouse_cb(m_mouse_user, kind, static_cast<int>(p.x()),
                   static_cast<int>(p.y()), btn);
    }

    QtPaintCb m_cb;
    void *m_user;
    QtMouseCb m_mouse_cb = nullptr;
    void *m_mouse_user = nullptr;
    QtResizeCb m_resize_cb = nullptr;
    void *m_resize_user = nullptr;
    QtWheelCb m_wheel_cb = nullptr;
    void *m_wheel_user = nullptr;
};

} // namespace

extern "C" {

// ---- Application -----------------------------------------------------------
// QApplication (not QGuiApplication) so the same app object serves both QML and
// the widget component tree; QApplication derives from QGuiApplication.
QtApp *qt_app_new() {
    return reinterpret_cast<QtApp *>(new QApplication(g_argc, g_argv));
}
int qt_app_exec(QtApp *) { return QApplication::exec(); }
int qt_app_run_for(QtApp *, int ms) {
    // Run the real event loop for a bounded time, then quit. Lets timers and
    // posted callbacks fire without blocking forever — used by tests and demos.
    QTimer::singleShot(ms, qApp, []() { QCoreApplication::quit(); });
    return QApplication::exec();
}
void qt_app_quit(QtApp *) { QApplication::quit(); }
void qt_app_delete(QtApp *app) {
    delete reinterpret_cast<QApplication *>(app);
}

// ---- QML engine ------------------------------------------------------------
QtEngine *qt_qml_engine_new() {
    return reinterpret_cast<QtEngine *>(new QQmlApplicationEngine());
}
void qt_qml_engine_load_file(QtEngine *e, const char *path) {
    reinterpret_cast<QQmlApplicationEngine *>(e)->load(
        QUrl::fromLocalFile(from_c(path)));
}
void qt_qml_engine_load_url(QtEngine *e, const char *url) {
    reinterpret_cast<QQmlApplicationEngine *>(e)->load(QUrl(from_c(url)));
}
void qt_qml_engine_load_data(QtEngine *e, const char *data, size_t len,
                             const char *url) {
    reinterpret_cast<QQmlApplicationEngine *>(e)->loadData(
        QByteArray(data, static_cast<qsizetype>(len)), QUrl(from_c(url)));
}
int qt_qml_engine_root_count(QtEngine *e) {
    return reinterpret_cast<QQmlApplicationEngine *>(e)->rootObjects().size();
}
void qt_qml_engine_set_context_object(QtEngine *e, const char *name,
                                      QtObject *obj) {
    auto *engine = reinterpret_cast<QQmlApplicationEngine *>(e);
    engine->rootContext()->setContextProperty(
        from_c(name), reinterpret_cast<QObject *>(obj));
}
void qt_qml_engine_delete(QtEngine *e) {
    delete reinterpret_cast<QQmlApplicationEngine *>(e);
}

// ---- QQmlPropertyMap -------------------------------------------------------
QtPropertyMap *qt_property_map_new() {
    return reinterpret_cast<QtPropertyMap *>(new QQmlPropertyMap());
}
QtObject *qt_property_map_as_object(QtPropertyMap *m) {
    return reinterpret_cast<QtObject *>(
        static_cast<QObject *>(reinterpret_cast<QQmlPropertyMap *>(m)));
}
void qt_property_map_set_i64(QtPropertyMap *m, const char *key, int64_t v) {
    reinterpret_cast<QQmlPropertyMap *>(m)->insert(
        from_c(key), QVariant::fromValue<qlonglong>(v));
}
void qt_property_map_set_f64(QtPropertyMap *m, const char *key, double v) {
    reinterpret_cast<QQmlPropertyMap *>(m)->insert(from_c(key), QVariant(v));
}
void qt_property_map_set_bool(QtPropertyMap *m, const char *key, int v) {
    reinterpret_cast<QQmlPropertyMap *>(m)->insert(from_c(key),
                                                   QVariant(v != 0));
}
void qt_property_map_set_str(QtPropertyMap *m, const char *key, const char *v) {
    reinterpret_cast<QQmlPropertyMap *>(m)->insert(from_c(key),
                                                   QVariant(from_c(v)));
}
QtVariantKind qt_property_map_kind(QtPropertyMap *m, const char *key) {
    QVariant val = reinterpret_cast<QQmlPropertyMap *>(m)->value(from_c(key));
    if (!val.isValid())
        return QT_VK_INVALID;
    switch (val.typeId()) {
    case QMetaType::Bool:
        return QT_VK_BOOL;
    case QMetaType::Int:
    case QMetaType::LongLong:
    case QMetaType::UInt:
    case QMetaType::ULongLong:
        return QT_VK_I64;
    case QMetaType::Double:
    case QMetaType::Float:
        return QT_VK_F64;
    default:
        return QT_VK_STRING;
    }
}
int64_t qt_property_map_get_i64(QtPropertyMap *m, const char *key) {
    return reinterpret_cast<QQmlPropertyMap *>(m)->value(from_c(key)).toLongLong();
}
double qt_property_map_get_f64(QtPropertyMap *m, const char *key) {
    return reinterpret_cast<QQmlPropertyMap *>(m)->value(from_c(key)).toDouble();
}
int qt_property_map_get_bool(QtPropertyMap *m, const char *key) {
    return reinterpret_cast<QQmlPropertyMap *>(m)->value(from_c(key)).toBool() ? 1
                                                                               : 0;
}
char *qt_property_map_get_str(QtPropertyMap *m, const char *key) {
    return dup_c(
        reinterpret_cast<QQmlPropertyMap *>(m)->value(from_c(key)).toString());
}
void qt_property_map_on_changed(QtPropertyMap *m, QtStrCb cb, void *user) {
    auto *map = reinterpret_cast<QQmlPropertyMap *>(m);
    QObject::connect(map, &QQmlPropertyMap::valueChanged, map,
                     [cb, user](const QString &key, const QVariant &) {
                         cb(user, key.toUtf8().constData());
                     });
}
void qt_property_map_delete(QtPropertyMap *m) {
    delete reinterpret_cast<QQmlPropertyMap *>(m);
}

// ---- Widgets base ----------------------------------------------------------
QtWidget *qt_widget_new() { return reinterpret_cast<QtWidget *>(new QWidget()); }
void qt_widget_delete(QtWidget *w) { delete W(w); }
void qt_widget_show(QtWidget *w) { W(w)->show(); }
void qt_widget_set_window_title(QtWidget *w, const char *title) {
    W(w)->setWindowTitle(from_c(title));
}
void qt_widget_resize(QtWidget *w, int width, int height) {
    W(w)->resize(width, height);
}
void qt_widget_set_layout(QtWidget *w, QtLayout *layout) {
    W(w)->setLayout(L(layout));
}
void qt_widget_set_enabled(QtWidget *w, int enabled) {
    W(w)->setEnabled(enabled != 0);
}
void qt_widget_set_fixed_size(QtWidget *w, int width, int height) {
    W(w)->setFixedSize(width, height);
}
void qt_widget_unset_fixed_size(QtWidget *w) {
    // Release a previously pinned size back to the layout: min back to 0 and max
    // back to Qt's unbounded sentinel (QWIDGETSIZE_MAX).
    W(w)->setMinimumSize(0, 0);
    W(w)->setMaximumSize(QWIDGETSIZE_MAX, QWIDGETSIZE_MAX);
}
void qt_widget_update(QtWidget *w) { W(w)->update(); }
void qt_widget_repaint(QtWidget *w) { W(w)->repaint(); }

// ---- custom-drawn widget ---------------------------------------------------
QtWidget *qt_canvas_new(QtPaintCb cb, void *user) {
    return reinterpret_cast<QtWidget *>(new RsCanvas(cb, user));
}
void qt_canvas_on_mouse(QtWidget *w, QtMouseCb cb, void *user, int track) {
    static_cast<RsCanvas *>(W(w))->setMouse(cb, user, track);
}
void qt_canvas_set_mouse_tracking(QtWidget *w, int track) {
    static_cast<RsCanvas *>(W(w))->setMouseTrack(track);
}
void qt_canvas_send_mouse(QtWidget *w, int kind, int x, int y, int button) {
    // Synthesize and deliver a mouse event straight to the widget. Used to drive
    // canvases from tests/automation without a real pointer device.
    QEvent::Type type = kind == 0   ? QEvent::MouseButtonPress
                        : kind == 2 ? QEvent::MouseButtonRelease
                                    : QEvent::MouseMove;
    Qt::MouseButton b = button == 1   ? Qt::LeftButton
                        : button == 2 ? Qt::RightButton
                        : button == 4 ? Qt::MiddleButton
                                      : Qt::NoButton;
    QMouseEvent ev(type, QPointF(x, y), QPointF(x, y), b, b, Qt::NoModifier);
    QCoreApplication::sendEvent(W(w), &ev);
}
void qt_canvas_on_resize(QtWidget *w, QtResizeCb cb, void *user) {
    static_cast<RsCanvas *>(W(w))->setResize(cb, user);
}
void qt_canvas_on_wheel(QtWidget *w, QtWheelCb cb, void *user) {
    static_cast<RsCanvas *>(W(w))->setWheel(cb, user);
}

// ---- painter: state, transforms, quality -----------------------------------
void qt_painter_save(QtPainter *p) { P(p)->save(); }
void qt_painter_restore(QtPainter *p) { P(p)->restore(); }
void qt_painter_translate(QtPainter *p, double dx, double dy) {
    P(p)->translate(dx, dy);
}
void qt_painter_rotate(QtPainter *p, double degrees) { P(p)->rotate(degrees); }
void qt_painter_scale(QtPainter *p, double sx, double sy) { P(p)->scale(sx, sy); }
void qt_painter_set_opacity(QtPainter *p, double opacity) {
    P(p)->setOpacity(opacity);
}
void qt_painter_set_antialiasing(QtPainter *p, int on) {
    P(p)->setRenderHint(QPainter::Antialiasing, on != 0);
    P(p)->setRenderHint(QPainter::SmoothPixmapTransform, on != 0);
}
void qt_painter_set_font(QtPainter *p, const char *family, int px, int bold) {
    QFont f(from_c(family), px);
    f.setBold(bold != 0);
    P(p)->setFont(f);
}

// ---- painter: extra shapes -------------------------------------------------
void qt_painter_stroke_ellipse(QtPainter *p, int x, int y, int w, int h,
                               int line, int r, int g, int b, int a) {
    QPainter *pp = P(p);
    QPen pen(QColor(r, g, b, a));
    pen.setWidth(line);
    pp->setPen(pen);
    pp->setBrush(Qt::NoBrush);
    pp->drawEllipse(QRect(x, y, w, h));
}
void qt_painter_fill_rounded_rect(QtPainter *p, int x, int y, int w, int h,
                                  double rx, double ry, int r, int g, int b,
                                  int a) {
    QPainter *pp = P(p);
    pp->setPen(Qt::NoPen);
    pp->setBrush(QColor(r, g, b, a));
    pp->drawRoundedRect(QRectF(x, y, w, h), rx, ry);
}
void qt_painter_stroke_rounded_rect(QtPainter *p, int x, int y, int w, int h,
                                    double rx, double ry, int line, int r, int g,
                                    int b, int a) {
    QPainter *pp = P(p);
    QPen pen(QColor(r, g, b, a));
    pen.setWidth(line);
    pp->setPen(pen);
    pp->setBrush(Qt::NoBrush);
    pp->drawRoundedRect(QRectF(x, y, w, h), rx, ry);
}
static QPolygon poly_from(const int *pts, int n) {
    QPolygon poly;
    poly.reserve(n);
    for (int i = 0; i < n; ++i)
        poly << QPoint(pts[2 * i], pts[2 * i + 1]);
    return poly;
}
void qt_painter_fill_polygon(QtPainter *p, const int *pts, int n, int r, int g,
                             int b, int a) {
    QPainter *pp = P(p);
    pp->setPen(Qt::NoPen);
    pp->setBrush(QColor(r, g, b, a));
    pp->drawPolygon(poly_from(pts, n));
}
void qt_painter_draw_polyline(QtPainter *p, const int *pts, int n, int line,
                              int r, int g, int b, int a) {
    QPainter *pp = P(p);
    QPen pen(QColor(r, g, b, a));
    pen.setWidth(line);
    pp->setPen(pen);
    pp->setBrush(Qt::NoBrush);
    pp->drawPolyline(poly_from(pts, n));
}

// ---- painter: gradient fills -----------------------------------------------
void qt_painter_fill_rect_lgrad(QtPainter *p, int x, int y, int w, int h,
                                double x1, double y1, double x2, double y2,
                                int r1, int g1, int b1, int a1, int r2, int g2,
                                int b2, int a2) {
    QLinearGradient grad(x1, y1, x2, y2);
    grad.setColorAt(0.0, QColor(r1, g1, b1, a1));
    grad.setColorAt(1.0, QColor(r2, g2, b2, a2));
    P(p)->fillRect(QRect(x, y, w, h), QBrush(grad));
}
void qt_painter_fill_rect_rgrad(QtPainter *p, int x, int y, int w, int h,
                                double cx, double cy, double radius, int r1,
                                int g1, int b1, int a1, int r2, int g2, int b2,
                                int a2) {
    QRadialGradient grad(cx, cy, radius);
    grad.setColorAt(0.0, QColor(r1, g1, b1, a1));
    grad.setColorAt(1.0, QColor(r2, g2, b2, a2));
    P(p)->fillRect(QRect(x, y, w, h), QBrush(grad));
}

// ---- painter path ----------------------------------------------------------
static QPainterPath *PP(QtPath *p) { return reinterpret_cast<QPainterPath *>(p); }
QtPath *qt_path_new() { return reinterpret_cast<QtPath *>(new QPainterPath()); }
void qt_path_move_to(QtPath *path, double x, double y) { PP(path)->moveTo(x, y); }
void qt_path_line_to(QtPath *path, double x, double y) { PP(path)->lineTo(x, y); }
void qt_path_cubic_to(QtPath *path, double c1x, double c1y, double c2x,
                      double c2y, double ex, double ey) {
    PP(path)->cubicTo(c1x, c1y, c2x, c2y, ex, ey);
}
void qt_path_close(QtPath *path) { PP(path)->closeSubpath(); }
void qt_path_delete(QtPath *path) { delete PP(path); }
void qt_painter_fill_path(QtPainter *p, QtPath *path, int r, int g, int b,
                          int a) {
    P(p)->fillPath(*PP(path), QColor(r, g, b, a));
}
void qt_painter_stroke_path(QtPainter *p, QtPath *path, int line, int r, int g,
                            int b, int a) {
    QPen pen(QColor(r, g, b, a));
    pen.setWidth(line);
    P(p)->strokePath(*PP(path), pen);
}
void qt_painter_clip_path(QtPainter *p, QtPath *path) {
    P(p)->setClipPath(*PP(path));
}

// ---- image -----------------------------------------------------------------
static QImage *IMG(QtImage *i) { return reinterpret_cast<QImage *>(i); }
QtImage *qt_image_load(const char *path) {
    auto *img = new QImage();
    if (!img->load(from_c(path))) {
        delete img;
        return nullptr;
    }
    return reinterpret_cast<QtImage *>(img);
}
QtImage *qt_image_from_data(const unsigned char *data, int len) {
    auto *img = new QImage();
    if (!img->loadFromData(data, len)) {
        delete img;
        return nullptr;
    }
    return reinterpret_cast<QtImage *>(img);
}
int qt_image_width(QtImage *i) { return IMG(i)->width(); }
int qt_image_height(QtImage *i) { return IMG(i)->height(); }
void qt_image_delete(QtImage *i) { delete IMG(i); }
void qt_painter_draw_image(QtPainter *p, QtImage *i, int x, int y) {
    P(p)->drawImage(QPoint(x, y), *IMG(i));
}
void qt_painter_draw_image_scaled(QtPainter *p, QtImage *i, int x, int y, int w,
                                  int h) {
    P(p)->drawImage(QRect(x, y, w, h), *IMG(i));
}
void qt_painter_fill_rect(QtPainter *p, int x, int y, int w, int h, int r, int g,
                          int b, int a) {
    P(p)->fillRect(QRect(x, y, w, h), QColor(r, g, b, a));
}
void qt_painter_stroke_rect(QtPainter *p, int x, int y, int w, int h, int line,
                            int r, int g, int b, int a) {
    QPainter *pp = P(p);
    QPen pen(QColor(r, g, b, a));
    pen.setWidth(line);
    pp->setPen(pen);
    pp->setBrush(Qt::NoBrush);
    pp->drawRect(QRect(x, y, w, h));
}
void qt_painter_fill_ellipse(QtPainter *p, int x, int y, int w, int h, int r,
                             int g, int b, int a) {
    QPainter *pp = P(p);
    pp->setPen(Qt::NoPen);
    pp->setBrush(QColor(r, g, b, a));
    pp->drawEllipse(QRect(x, y, w, h));
}
void qt_painter_draw_line(QtPainter *p, int x1, int y1, int x2, int y2, int line,
                          int r, int g, int b, int a) {
    QPainter *pp = P(p);
    QPen pen(QColor(r, g, b, a));
    pen.setWidth(line);
    pp->setPen(pen);
    pp->drawLine(x1, y1, x2, y2);
}
void qt_painter_draw_text(QtPainter *p, int x, int y, const char *s, int r,
                          int g, int b, int a) {
    QPainter *pp = P(p);
    pp->setPen(QColor(r, g, b, a));
    pp->drawText(x, y, from_c(s));
}
int qt_widget_block_signals(QtWidget *w, int block) {
    return W(w)->blockSignals(block != 0) ? 1 : 0;
}
void qt_post(QtVoidCb cb, void *user) {
    QTimer::singleShot(0, qApp, [cb, user]() { cb(user); });
}
void qt_post_to_main(QtVoidCb cb, void *user) {
    // Thread-safe: schedules `cb` to run on the GUI thread. Unlike qt_post this
    // may be called from any thread (QueuedConnection marshals across threads),
    // so background work can feed messages back into the reactive runtime.
    QMetaObject::invokeMethod(
        qApp, [cb, user]() { cb(user); }, Qt::QueuedConnection);
}

QtWidget *qt_label_new(const char *text) {
    return reinterpret_cast<QtWidget *>(new QLabel(from_c(text)));
}
void qt_label_set_text(QtWidget *label, const char *text) {
    static_cast<QLabel *>(W(label))->setText(from_c(text));
}

QtWidget *qt_button_new(const char *text) {
    return reinterpret_cast<QtWidget *>(new QPushButton(from_c(text)));
}
void qt_button_set_text(QtWidget *button, const char *text) {
    static_cast<QPushButton *>(W(button))->setText(from_c(text));
}
void qt_button_on_clicked(QtWidget *button, QtVoidCb cb, void *user) {
    auto *b = static_cast<QPushButton *>(W(button));
    QObject::connect(b, &QPushButton::clicked, b, [cb, user]() { cb(user); });
}

QtLayout *qt_box_layout_new(int vertical) {
    QBoxLayout *layout = vertical
                             ? static_cast<QBoxLayout *>(new QVBoxLayout())
                             : static_cast<QBoxLayout *>(new QHBoxLayout());
    return reinterpret_cast<QtLayout *>(layout);
}
void qt_layout_add_widget(QtLayout *layout, QtWidget *child) {
    L(layout)->addWidget(W(child));
}
void qt_layout_add_layout(QtLayout *layout, QtLayout *child) {
    static_cast<QBoxLayout *>(L(layout))->addLayout(L(child));
}
void qt_layout_add_stretch(QtLayout *layout) {
    static_cast<QBoxLayout *>(L(layout))->addStretch();
}
void qt_layout_set_spacing(QtLayout *layout, int spacing) {
    L(layout)->setSpacing(spacing);
}
void qt_layout_set_margins(QtLayout *layout, int l, int t, int r, int b) {
    L(layout)->setContentsMargins(l, t, r, b);
}
void qt_layout_insert_widget(QtLayout *layout, int index, QtWidget *child) {
    static_cast<QBoxLayout *>(L(layout))->insertWidget(index, W(child));
}
void qt_layout_insert_layout(QtLayout *layout, int index, QtLayout *child) {
    static_cast<QBoxLayout *>(L(layout))->insertLayout(index, L(child));
}
void qt_layout_insert_stretch(QtLayout *layout, int index) {
    static_cast<QBoxLayout *>(L(layout))->insertStretch(index);
}
void qt_layout_remove_at(QtLayout *layout, int index) {
    if (QLayoutItem *item = L(layout)->takeAt(index)) {
        if (QWidget *w = item->widget())
            w->deleteLater();
        if (QLayout *child = item->layout())
            child->deleteLater();
        delete item;
    }
}
void qt_layout_clear(QtLayout *layout) {
    QLayout *l = L(layout);
    while (QLayoutItem *item = l->takeAt(0)) {
        if (QWidget *w = item->widget())
            w->deleteLater();
        if (QLayout *child = item->layout())
            child->deleteLater();
        delete item;
    }
}

// ---- checkbox --------------------------------------------------------------
QtWidget *qt_checkbox_new(const char *text) {
    return reinterpret_cast<QtWidget *>(new QCheckBox(from_c(text)));
}
void qt_checkbox_set_text(QtWidget *w, const char *text) {
    static_cast<QCheckBox *>(W(w))->setText(from_c(text));
}
void qt_checkbox_set_checked(QtWidget *w, int checked) {
    static_cast<QCheckBox *>(W(w))->setChecked(checked != 0);
}
int qt_checkbox_is_checked(QtWidget *w) {
    return static_cast<QCheckBox *>(W(w))->isChecked() ? 1 : 0;
}
void qt_checkbox_on_toggled(QtWidget *w, QtBoolCb cb, void *user) {
    auto *c = static_cast<QCheckBox *>(W(w));
    QObject::connect(c, &QCheckBox::toggled, c,
                     [cb, user](bool on) { cb(user, on ? 1 : 0); });
}

// ---- line edit -------------------------------------------------------------
QtWidget *qt_line_edit_new(const char *text) {
    return reinterpret_cast<QtWidget *>(new QLineEdit(from_c(text)));
}
void qt_line_edit_set_text(QtWidget *w, const char *text) {
    static_cast<QLineEdit *>(W(w))->setText(from_c(text));
}
char *qt_line_edit_text(QtWidget *w) {
    return dup_c(static_cast<QLineEdit *>(W(w))->text());
}
void qt_line_edit_set_placeholder(QtWidget *w, const char *text) {
    static_cast<QLineEdit *>(W(w))->setPlaceholderText(from_c(text));
}
void qt_line_edit_on_changed(QtWidget *w, QtStrCb cb, void *user) {
    auto *e = static_cast<QLineEdit *>(W(w));
    QObject::connect(e, &QLineEdit::textChanged, e, [cb, user](const QString &t) {
        cb(user, t.toUtf8().constData());
    });
}

// ---- slider ----------------------------------------------------------------
QtWidget *qt_slider_new(int min, int max, int value) {
    auto *s = new QSlider(Qt::Horizontal);
    s->setRange(min, max);
    s->setValue(value);
    return reinterpret_cast<QtWidget *>(s);
}
void qt_slider_set_value(QtWidget *w, int value) {
    static_cast<QSlider *>(W(w))->setValue(value);
}
int qt_slider_value(QtWidget *w) {
    return static_cast<QSlider *>(W(w))->value();
}
void qt_slider_on_changed(QtWidget *w, QtIntCb cb, void *user) {
    auto *s = static_cast<QSlider *>(W(w));
    QObject::connect(s, &QSlider::valueChanged, s,
                     [cb, user](int v) { cb(user, v); });
}

// ---- spinbox ---------------------------------------------------------------
QtWidget *qt_spinbox_new(int min, int max, int value) {
    auto *s = new QSpinBox();
    s->setRange(min, max);
    s->setValue(value);
    return reinterpret_cast<QtWidget *>(s);
}
void qt_spinbox_set_value(QtWidget *w, int value) {
    static_cast<QSpinBox *>(W(w))->setValue(value);
}
int qt_spinbox_value(QtWidget *w) {
    return static_cast<QSpinBox *>(W(w))->value();
}
void qt_spinbox_on_changed(QtWidget *w, QtIntCb cb, void *user) {
    auto *s = static_cast<QSpinBox *>(W(w));
    QObject::connect(s, &QSpinBox::valueChanged, s,
                     [cb, user](int v) { cb(user, v); });
}

// ---- progress bar ----------------------------------------------------------
QtWidget *qt_progress_bar_new(int min, int max, int value) {
    auto *p = new QProgressBar();
    p->setRange(min, max);
    p->setValue(value);
    return reinterpret_cast<QtWidget *>(p);
}
void qt_progress_bar_set_value(QtWidget *w, int value) {
    static_cast<QProgressBar *>(W(w))->setValue(value);
}

// ---- combo box -------------------------------------------------------------
QtWidget *qt_combo_box_new() {
    return reinterpret_cast<QtWidget *>(new QComboBox());
}
void qt_combo_box_add_item(QtWidget *w, const char *text) {
    static_cast<QComboBox *>(W(w))->addItem(from_c(text));
}
void qt_combo_box_clear(QtWidget *w) {
    static_cast<QComboBox *>(W(w))->clear();
}
int qt_combo_box_current_index(QtWidget *w) {
    return static_cast<QComboBox *>(W(w))->currentIndex();
}
void qt_combo_box_set_current_index(QtWidget *w, int index) {
    static_cast<QComboBox *>(W(w))->setCurrentIndex(index);
}
void qt_combo_box_on_changed(QtWidget *w, QtIntCb cb, void *user) {
    auto *c = static_cast<QComboBox *>(W(w));
    QObject::connect(c, &QComboBox::currentIndexChanged, c,
                     [cb, user](int i) { cb(user, i); });
}

// ---- list widget -----------------------------------------------------------
QtWidget *qt_list_new() { return reinterpret_cast<QtWidget *>(new QListWidget()); }
void qt_list_add_item(QtWidget *w, const char *text) {
    static_cast<QListWidget *>(W(w))->addItem(from_c(text));
}
void qt_list_clear(QtWidget *w) { static_cast<QListWidget *>(W(w))->clear(); }
int qt_list_current_row(QtWidget *w) {
    return static_cast<QListWidget *>(W(w))->currentRow();
}
void qt_list_set_current_row(QtWidget *w, int row) {
    static_cast<QListWidget *>(W(w))->setCurrentRow(row);
}
void qt_list_on_current_changed(QtWidget *w, QtIntCb cb, void *user) {
    auto *l = static_cast<QListWidget *>(W(w));
    QObject::connect(l, &QListWidget::currentRowChanged, l,
                     [cb, user](int row) { cb(user, row); });
}
void qt_list_on_activated(QtWidget *w, QtIntCb cb, void *user) {
    auto *l = static_cast<QListWidget *>(W(w));
    QObject::connect(l, &QListWidget::itemActivated, l,
                     [cb, user, l](QListWidgetItem *it) { cb(user, l->row(it)); });
}

// ---- main window + menus ---------------------------------------------------
QtWidget *qt_main_window_new() {
    return reinterpret_cast<QtWidget *>(new QMainWindow());
}
void qt_main_window_set_central(QtWidget *mw, QtWidget *central) {
    static_cast<QMainWindow *>(W(mw))->setCentralWidget(W(central));
}
void qt_main_window_set_status(QtWidget *mw, const char *text) {
    static_cast<QMainWindow *>(W(mw))->statusBar()->showMessage(from_c(text));
}
QtMenu *qt_main_window_add_menu(QtWidget *mw, const char *title) {
    auto *bar = static_cast<QMainWindow *>(W(mw))->menuBar();
    return reinterpret_cast<QtMenu *>(bar->addMenu(from_c(title)));
}
void qt_menu_add_action(QtMenu *menu, const char *text, QtVoidCb cb,
                        void *user) {
    auto *m = reinterpret_cast<QMenu *>(menu);
    QAction *act = m->addAction(from_c(text));
    QObject::connect(act, &QAction::triggered, act, [cb, user]() { cb(user); });
}
void qt_menu_add_separator(QtMenu *menu) {
    reinterpret_cast<QMenu *>(menu)->addSeparator();
}
QtMenu *qt_menu_add_submenu(QtMenu *menu, const char *title) {
    return reinterpret_cast<QtMenu *>(
        reinterpret_cast<QMenu *>(menu)->addMenu(from_c(title)));
}

// ---- dialogs (modal, imperative) -------------------------------------------
void qt_dialog_message(const char *title, const char *text) {
    QMessageBox::information(nullptr, from_c(title), from_c(text));
}
int qt_dialog_confirm(const char *title, const char *text) {
    auto r = QMessageBox::question(nullptr, from_c(title), from_c(text),
                                   QMessageBox::Yes | QMessageBox::No);
    return r == QMessageBox::Yes ? 1 : 0;
}
// Returns a malloc'd string (caller frees with qt_string_free) or NULL if the
// user cancelled.
char *qt_dialog_input(const char *title, const char *label, const char *initial) {
    bool ok = false;
    QString text = QInputDialog::getText(nullptr, from_c(title), from_c(label),
                                         QLineEdit::Normal, from_c(initial), &ok);
    return ok ? dup_c(text) : nullptr;
}

// ---- file / directory choosers (modal, imperative) -------------------------
// Each returns a malloc'd UTF-8 path the caller frees with qt_string_free, or
// NULL if the user cancelled. `dir` / `filter` may be NULL/empty.
char *qt_dialog_open_file(const char *title, const char *dir,
                          const char *filter) {
    QString path = QFileDialog::getOpenFileName(nullptr, from_c(title),
                                                from_c(dir), from_c(filter));
    return path.isNull() ? nullptr : dup_c(path);
}
char *qt_dialog_save_file(const char *title, const char *dir,
                          const char *filter) {
    QString path = QFileDialog::getSaveFileName(nullptr, from_c(title),
                                                from_c(dir), from_c(filter));
    return path.isNull() ? nullptr : dup_c(path);
}
char *qt_dialog_open_dir(const char *title, const char *dir) {
    QString path = QFileDialog::getExistingDirectory(nullptr, from_c(title),
                                                     from_c(dir));
    return path.isNull() ? nullptr : dup_c(path);
}

// ---- popup / context menu (modal, imperative) ------------------------------
// Shows a menu of `items` at global (x, y) and returns the chosen 0-based index,
// or -1 if dismissed. Handy for right-click context menus on a canvas.
int qt_popup_menu(const char *const *items, int n, int x, int y) {
    QMenu menu;
    for (int i = 0; i < n; ++i)
        menu.addAction(from_c(items[i]))->setData(i);
    QAction *chosen = menu.exec(QPoint(x, y));
    return chosen ? chosen->data().toInt() : -1;
}

// ---- radio button ----------------------------------------------------------
QtWidget *qt_radio_button_new(const char *text) {
    return reinterpret_cast<QtWidget *>(new QRadioButton(from_c(text)));
}
void qt_radio_button_set_text(QtWidget *w, const char *text) {
    static_cast<QRadioButton *>(W(w))->setText(from_c(text));
}
void qt_radio_button_set_checked(QtWidget *w, int checked) {
    static_cast<QRadioButton *>(W(w))->setChecked(checked != 0);
}
int qt_radio_button_is_checked(QtWidget *w) {
    return static_cast<QRadioButton *>(W(w))->isChecked() ? 1 : 0;
}
void qt_radio_button_on_toggled(QtWidget *w, QtBoolCb cb, void *user) {
    auto *r = static_cast<QRadioButton *>(W(w));
    QObject::connect(r, &QRadioButton::toggled, r,
                     [cb, user](bool on) { cb(user, on ? 1 : 0); });
}

// ---- multi-line text edit --------------------------------------------------
QtWidget *qt_text_edit_new(const char *text) {
    return reinterpret_cast<QtWidget *>(new QPlainTextEdit(from_c(text)));
}
void qt_text_edit_set_text(QtWidget *w, const char *text) {
    static_cast<QPlainTextEdit *>(W(w))->setPlainText(from_c(text));
}
char *qt_text_edit_text(QtWidget *w) {
    return dup_c(static_cast<QPlainTextEdit *>(W(w))->toPlainText());
}
void qt_text_edit_set_placeholder(QtWidget *w, const char *text) {
    static_cast<QPlainTextEdit *>(W(w))->setPlaceholderText(from_c(text));
}
void qt_text_edit_set_read_only(QtWidget *w, int read_only) {
    static_cast<QPlainTextEdit *>(W(w))->setReadOnly(read_only != 0);
}
void qt_text_edit_on_changed(QtWidget *w, QtStrCb cb, void *user) {
    auto *e = static_cast<QPlainTextEdit *>(W(w));
    QObject::connect(e, &QPlainTextEdit::textChanged, e, [cb, user, e]() {
        cb(user, e->toPlainText().toUtf8().constData());
    });
}

// ---- dial ------------------------------------------------------------------
QtWidget *qt_dial_new(int min, int max, int value) {
    auto *d = new QDial();
    d->setRange(min, max);
    d->setValue(value);
    return reinterpret_cast<QtWidget *>(d);
}
void qt_dial_set_value(QtWidget *w, int value) {
    static_cast<QDial *>(W(w))->setValue(value);
}
int qt_dial_value(QtWidget *w) {
    return static_cast<QDial *>(W(w))->value();
}
void qt_dial_on_changed(QtWidget *w, QtIntCb cb, void *user) {
    auto *d = static_cast<QDial *>(W(w));
    QObject::connect(d, &QDial::valueChanged, d,
                     [cb, user](int v) { cb(user, v); });
}

// ---- double spin box -------------------------------------------------------
QtWidget *qt_double_spinbox_new(double min, double max, double value,
                                int decimals, double step) {
    auto *s = new QDoubleSpinBox();
    s->setRange(min, max);
    s->setDecimals(decimals);
    s->setSingleStep(step);
    s->setValue(value);
    return reinterpret_cast<QtWidget *>(s);
}
void qt_double_spinbox_set_value(QtWidget *w, double value) {
    static_cast<QDoubleSpinBox *>(W(w))->setValue(value);
}
double qt_double_spinbox_value(QtWidget *w) {
    return static_cast<QDoubleSpinBox *>(W(w))->value();
}
void qt_double_spinbox_on_changed(QtWidget *w, QtDoubleCb cb, void *user) {
    auto *s = static_cast<QDoubleSpinBox *>(W(w));
    QObject::connect(s, &QDoubleSpinBox::valueChanged, s,
                     [cb, user](double v) { cb(user, v); });
}

// ---- group box -------------------------------------------------------------
// A titled frame that hosts a child layout. attach() treats it as a plain
// widget; the reactive diff owns the inner layout it is given via set_layout.
QtWidget *qt_group_box_new(const char *title) {
    return reinterpret_cast<QtWidget *>(new QGroupBox(from_c(title)));
}
void qt_group_box_set_title(QtWidget *w, const char *title) {
    static_cast<QGroupBox *>(W(w))->setTitle(from_c(title));
}

// ---- separator (horizontal / vertical rule) --------------------------------
QtWidget *qt_separator_new(int vertical) {
    auto *f = new QFrame();
    f->setFrameShape(vertical ? QFrame::VLine : QFrame::HLine);
    f->setFrameShadow(QFrame::Sunken);
    return reinterpret_cast<QtWidget *>(f);
}

// ---- timer -----------------------------------------------------------------
static QTimer *T(QtTimer *t) { return reinterpret_cast<QTimer *>(t); }
QtTimer *qt_timer_new(int interval_ms, QtVoidCb cb, void *user) {
    auto *t = new QTimer();
    t->setInterval(interval_ms);
    QObject::connect(t, &QTimer::timeout, t, [cb, user]() { cb(user); });
    t->start();
    return reinterpret_cast<QtTimer *>(t);
}
void qt_timer_set_interval(QtTimer *t, int interval_ms) {
    T(t)->setInterval(interval_ms);
}
void qt_timer_start(QtTimer *t) { T(t)->start(); }
void qt_timer_stop(QtTimer *t) { T(t)->stop(); }
void qt_timer_delete(QtTimer *t) { delete T(t); }

// ---- i18n / resources ------------------------------------------------------
char *qt_translate(const char *context, const char *source) {
    return dup_c(QCoreApplication::translate(context, source));
}
QtTranslator *qt_translator_load(const char *qm_path) {
    auto *t = new QTranslator();
    if (t->load(from_c(qm_path)) &&
        QCoreApplication::installTranslator(t)) {
        return reinterpret_cast<QtTranslator *>(t);
    }
    delete t;
    return nullptr;
}
int qt_resource_register(const unsigned char *data) {
    return QResource::registerResource(data) ? 1 : 0;
}

// ---- misc ------------------------------------------------------------------
void qt_string_free(char *s) { std::free(s); }

} // extern "C"
