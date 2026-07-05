#include "shim.h"

#include <QApplication>
#include <QBoxLayout>
#include <QCoreApplication>
#include <QByteArray>
#include <QCheckBox>
#include <QComboBox>
#include <QDial>
#include <QDoubleSpinBox>
#include <QFrame>
#include <QGroupBox>
#include <QGuiApplication>
#include <QLabel>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QRadioButton>
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

protected:
    void paintEvent(QPaintEvent *) override {
        if (!m_cb)
            return;
        QPainter painter(this);
        m_cb(m_user, reinterpret_cast<QtPainter *>(&painter), width(), height());
    }

private:
    QtPaintCb m_cb;
    void *m_user;
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
