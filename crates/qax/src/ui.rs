//! Declarative, state-driven UI built from Rust code — no QML, no handles.
//!
//! This layer follows the Elm architecture. You never touch a widget directly:
//!
//! 1. Hold your app's data in a [`Component`] and define [`Component::view`],
//!    a pure function from that data to an [`Element`] tree.
//! 2. UI events turn into your `Message` type; [`Component::update`] applies a
//!    message to the data.
//!
//! After every batch of messages the runtime calls `view` again, **diffs** the
//! fresh tree against the one on screen, and mutates only the widgets that
//! actually changed. You describe *what the UI should be* for the current state;
//! the library figures out the minimal set of Qt calls to get there.
//!
//! ```no_run
//! use qax::ui::*;
//! use qax::Application;
//!
//! #[derive(Clone)]
//! enum Msg { Inc, Dec }
//!
//! #[derive(Default)]
//! struct Counter { n: i64 }
//!
//! impl Component for Counter {
//!     type Message = Msg;
//!     fn update(&mut self, msg: Msg) {
//!         match msg {
//!             Msg::Inc => self.n += 1,
//!             Msg::Dec => self.n -= 1,
//!         }
//!     }
//!     fn view(&self) -> Element<Msg> {
//!         column().spacing(8).padding(16)
//!             .child(label(format!("Count: {}", self.n)))
//!             .child(row().spacing(8)
//!                 .child(button("−").on_click(Msg::Dec))
//!                 .child(button("+").on_click(Msg::Inc)))
//!             .into_element()
//!     }
//! }
//!
//! let app = Application::new();
//! let _ui = Ui::new(Counter::default()).title("counter").size(240, 140).mount();
//! app.exec();
//! ```

use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::os::raw::{c_char, c_int, c_void};
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use qax_sys as sys;

fn cstr(s: &str) -> CString {
    CString::new(s).expect("UI string contains NUL")
}

/// An event handler mapping a borrowed string (e.g. a line-edit's new text) to a
/// message. Shared (`Rc`) so it survives being cloned into diff closures.
type StrHandler<Msg> = Rc<dyn Fn(&str) -> Msg>;

/// Runs `set` on `w` with the widget's signals blocked, so a programmatic change
/// the diff makes does not echo back as a user event and cause a feedback loop.
fn quietly(w: *mut sys::QtWidget, set: impl FnOnce()) {
    unsafe {
        let prev = sys::qt_widget_block_signals(w, 1);
        set();
        sys::qt_widget_block_signals(w, prev);
    }
}

// ===========================================================================
// Message dispatch
// ===========================================================================

/// A per-widget callback slot. Qt holds a stable raw pointer to it; the diff can
/// swap the boxed closure in place (via the `RefCell`) when a handler changes,
/// without ever reconnecting the Qt signal.
struct Slot<A>(RefCell<Box<dyn Fn(A)>>);

extern "C" fn tramp_void(user: *mut c_void) {
    let slot = unsafe { &*(user as *const Slot<()>) };
    (slot.0.borrow())(());
}
extern "C" fn tramp_int(user: *mut c_void, v: c_int) {
    let slot = unsafe { &*(user as *const Slot<i32>) };
    (slot.0.borrow())(v);
}
extern "C" fn tramp_double(user: *mut c_void, v: f64) {
    let slot = unsafe { &*(user as *const Slot<f64>) };
    (slot.0.borrow())(v);
}
extern "C" fn tramp_bool(user: *mut c_void, v: c_int) {
    let slot = unsafe { &*(user as *const Slot<bool>) };
    (slot.0.borrow())(v != 0);
}
extern "C" fn tramp_str(user: *mut c_void, s: *const c_char) {
    let slot = unsafe { &*(user as *const Slot<String>) };
    let text = unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned();
    (slot.0.borrow())(text);
}

/// Drains the queue and re-renders. Invoked from the event loop via [`sys::qt_post`].
extern "C" fn tramp_flush(user: *mut c_void) {
    let flush = unsafe { &*(user as *const Box<dyn Fn()>) };
    flush();
}

/// Holds the current [`CustomWidget`] behind a stable pointer the canvas widget
/// keeps. The diff swaps the boxed widget in place, then requests a repaint.
struct CanvasSlot(RefCell<Box<dyn CustomWidget>>);

/// Qt paint-event trampoline: wraps the raw painter into a safe [`Canvas`] and
/// forwards to the current custom widget's `draw`.
extern "C" fn tramp_paint(user: *mut c_void, p: *mut sys::QtPainter, w: c_int, h: c_int) {
    let slot = unsafe { &*(user as *const CanvasSlot) };
    let mut canvas = Canvas {
        p,
        width: w,
        height: h,
    };
    slot.0.borrow().draw(&mut canvas);
}

/// Qt mouse-event trampoline: packs the raw ints into a [`MouseRaw`] and forwards
/// to the canvas's mouse slot, which routes to the right handler.
extern "C" fn tramp_mouse(user: *mut c_void, kind: c_int, x: c_int, y: c_int, button: c_int) {
    let slot = unsafe { &*(user as *const Slot<MouseRaw>) };
    (slot.0.borrow())(MouseRaw {
        kind,
        x,
        y,
        button,
    });
}

/// Qt wheel-event trampoline: forwards to the canvas's wheel slot.
extern "C" fn tramp_wheel(user: *mut c_void, x: c_int, y: c_int, delta: c_int) {
    let slot = unsafe { &*(user as *const Slot<WheelEvent>) };
    (slot.0.borrow())(WheelEvent { x, y, delta });
}

/// Qt resize-event trampoline: forwards the new `(w, h)` to the canvas's resize slot.
extern "C" fn tramp_resize(user: *mut c_void, w: c_int, h: c_int) {
    let slot = unsafe { &*(user as *const Slot<(i32, i32)>) };
    (slot.0.borrow())((w, h));
}

/// The channel a widget's event handler uses to feed messages back to the app.
/// Cloning is cheap (shared refs + a raw pointer) so every handler gets one.
struct Dispatch<Msg> {
    queue: Rc<RefCell<VecDeque<Msg>>>,
    scheduled: Rc<Cell<bool>>,
    flush: *const c_void,
}

impl<Msg> Clone for Dispatch<Msg> {
    fn clone(&self) -> Self {
        Dispatch {
            queue: self.queue.clone(),
            scheduled: self.scheduled.clone(),
            flush: self.flush,
        }
    }
}

impl<Msg: 'static> Dispatch<Msg> {
    /// Queues a message and, unless a flush is already pending, schedules one on
    /// the next event-loop turn. Deferring keeps the re-render off the stack of
    /// the signal handler that produced the message.
    fn emit(&self, msg: Msg) {
        self.queue.borrow_mut().push_back(msg);
        if !self.scheduled.replace(true) {
            unsafe { sys::qt_post(tramp_flush, self.flush as *mut c_void) };
        }
    }
}

// ===========================================================================
// Component
// ===========================================================================

/// Your application state, rendered reactively. Implement this, hand it to
/// [`Ui::new`], and the runtime keeps the widget tree in sync with your data.
pub trait Component: 'static {
    /// Values produced by UI events and consumed by [`Component::update`].
    type Message: Clone + 'static;

    /// Applies one message to the state. Called once per queued message before a
    /// re-render; keep it pure Rust (no Qt).
    fn update(&mut self, msg: Self::Message);

    /// Describes the UI for the current state. Called after each update batch;
    /// the returned tree is diffed against what is on screen.
    fn view(&self) -> Element<Self::Message>;

    /// Declares the timers that should be running for the current state. Called
    /// after every update batch, right after [`Component::view`]; the returned
    /// list is diffed against the live timers **by position**, so a subscription
    /// runs exactly as long as it stays in the list. Return an empty list (the
    /// default) for a UI with no timers.
    ///
    /// Include a timer only while it should tick — e.g. drive a 60 fps animation
    /// while playing, and drop it from the list when paused:
    ///
    /// ```ignore
    /// fn subscriptions(&self) -> Vec<Subscription<Msg>> {
    ///     if self.playing {
    ///         vec![every(Duration::from_millis(16), Msg::Tick)]
    ///     } else {
    ///         vec![]
    ///     }
    /// }
    /// ```
    fn subscriptions(&self) -> Vec<Subscription<Self::Message>> {
        Vec::new()
    }
}

/// A running timer declared from [`Component::subscriptions`]. Build one with
/// [`every`] (a fixed message) or [`every_with`] (a message computed per tick).
pub struct Subscription<Msg> {
    interval_ms: u64,
    make: Rc<dyn Fn() -> Msg>,
}

/// Emits `msg` every `interval`. The interval is truncated to whole
/// milliseconds (Qt's timer resolution). See [`Component::subscriptions`].
pub fn every<Msg: Clone + 'static>(interval: Duration, msg: Msg) -> Subscription<Msg> {
    Subscription {
        interval_ms: interval.as_millis() as u64,
        make: Rc::new(move || msg.clone()),
    }
}

/// Like [`every`], but computes the message afresh on each tick — handy when it
/// should carry a timestamp, a frame counter, or any live value.
pub fn every_with<Msg, F>(interval: Duration, f: F) -> Subscription<Msg>
where
    F: Fn() -> Msg + 'static,
{
    Subscription {
        interval_ms: interval.as_millis() as u64,
        make: Rc::new(f),
    }
}

// ===========================================================================
// Element tree (the "virtual" description a view produces)
// ===========================================================================

/// One node in the declarative UI description. Build these with [`label`],
/// [`button`], [`column`], … and nest them with `.child(..)`.
pub enum Element<Msg> {
    Label(LabelEl),
    Button(ButtonEl<Msg>),
    Checkbox(CheckboxEl<Msg>),
    RadioButton(RadioButtonEl<Msg>),
    LineEdit(LineEditEl<Msg>),
    TextEdit(TextEditEl<Msg>),
    Slider(SliderEl<Msg>),
    Dial(DialEl<Msg>),
    SpinBox(SpinBoxEl<Msg>),
    DoubleSpinBox(DoubleSpinBoxEl<Msg>),
    ProgressBar(ProgressBarEl),
    ComboBox(ComboBoxEl<Msg>),
    List(ListEl<Msg>),
    Separator(SeparatorEl),
    Container(ContainerEl<Msg>),
    GroupBox(GroupBoxEl<Msg>),
    /// A user-defined widget (see [`CustomWidget`]).
    Custom(CustomEl<Msg>),
    /// A flexible spacer; only meaningful inside a container.
    Stretch,
}

/// Anything that can become an [`Element`], so `.child()` takes builders directly.
pub trait IntoElement<Msg> {
    fn into_element(self) -> Element<Msg>;
}
impl<Msg> IntoElement<Msg> for Element<Msg> {
    fn into_element(self) -> Element<Msg> {
        self
    }
}

// ---- leaf builders --------------------------------------------------------

/// A text label.
pub struct LabelEl {
    text: String,
}
pub fn label(text: impl Into<String>) -> LabelEl {
    LabelEl { text: text.into() }
}
impl<Msg> IntoElement<Msg> for LabelEl {
    fn into_element(self) -> Element<Msg> {
        Element::Label(self)
    }
}

/// A push button. `on_click` names the message emitted when it is pressed.
pub struct ButtonEl<Msg> {
    text: String,
    on_click: Option<Msg>,
}
pub fn button<Msg>(text: impl Into<String>) -> ButtonEl<Msg> {
    ButtonEl {
        text: text.into(),
        on_click: None,
    }
}
impl<Msg> ButtonEl<Msg> {
    pub fn on_click(mut self, msg: Msg) -> Self {
        self.on_click = Some(msg);
        self
    }
}
impl<Msg> IntoElement<Msg> for ButtonEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::Button(self)
    }
}

/// A labelled checkbox. `on_toggle` maps the new checked state to a message.
pub struct CheckboxEl<Msg> {
    text: String,
    checked: bool,
    on_toggle: Option<Rc<dyn Fn(bool) -> Msg>>,
}
pub fn checkbox<Msg>(text: impl Into<String>) -> CheckboxEl<Msg> {
    CheckboxEl {
        text: text.into(),
        checked: false,
        on_toggle: None,
    }
}
impl<Msg> CheckboxEl<Msg> {
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
    pub fn on_toggle(mut self, f: impl Fn(bool) -> Msg + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for CheckboxEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::Checkbox(self)
    }
}

/// A radio button. Put several in the same container to form an exclusive group
/// (Qt makes radio buttons that share a parent mutually exclusive). `on_toggle`
/// maps the new checked state to a message.
pub struct RadioButtonEl<Msg> {
    text: String,
    checked: bool,
    on_toggle: Option<Rc<dyn Fn(bool) -> Msg>>,
}
pub fn radio_button<Msg>(text: impl Into<String>) -> RadioButtonEl<Msg> {
    RadioButtonEl {
        text: text.into(),
        checked: false,
        on_toggle: None,
    }
}
impl<Msg> RadioButtonEl<Msg> {
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
    pub fn on_toggle(mut self, f: impl Fn(bool) -> Msg + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for RadioButtonEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::RadioButton(self)
    }
}

/// A single-line text field. `on_change` maps the new text to a message.
pub struct LineEditEl<Msg> {
    text: String,
    placeholder: Option<String>,
    on_change: Option<StrHandler<Msg>>,
}
pub fn line_edit<Msg>() -> LineEditEl<Msg> {
    LineEditEl {
        text: String::new(),
        placeholder: None,
        on_change: None,
    }
}
impl<Msg> LineEditEl<Msg> {
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }
    pub fn on_change(mut self, f: impl Fn(&str) -> Msg + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for LineEditEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::LineEdit(self)
    }
}

/// A multi-line plain-text editor. `on_change` maps the new full text to a
/// message. Set `read_only` to use it purely as a scrollable text display.
pub struct TextEditEl<Msg> {
    text: String,
    placeholder: Option<String>,
    read_only: bool,
    on_change: Option<StrHandler<Msg>>,
}
pub fn text_edit<Msg>() -> TextEditEl<Msg> {
    TextEditEl {
        text: String::new(),
        placeholder: None,
        read_only: false,
        on_change: None,
    }
}
impl<Msg> TextEditEl<Msg> {
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = Some(text.into());
        self
    }
    pub fn read_only(mut self, on: bool) -> Self {
        self.read_only = on;
        self
    }
    pub fn on_change(mut self, f: impl Fn(&str) -> Msg + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for TextEditEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::TextEdit(self)
    }
}

/// A horizontal integer slider. `on_change` maps the value to a message.
pub struct SliderEl<Msg> {
    min: i32,
    max: i32,
    value: i32,
    on_change: Option<Rc<dyn Fn(i32) -> Msg>>,
}
pub fn slider<Msg>(min: i32, max: i32, value: i32) -> SliderEl<Msg> {
    SliderEl {
        min,
        max,
        value,
        on_change: None,
    }
}
impl<Msg> SliderEl<Msg> {
    pub fn on_change(mut self, f: impl Fn(i32) -> Msg + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for SliderEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::Slider(self)
    }
}

/// A rotary integer dial (like a knob). `on_change` maps the value to a message.
pub struct DialEl<Msg> {
    min: i32,
    max: i32,
    value: i32,
    on_change: Option<Rc<dyn Fn(i32) -> Msg>>,
}
pub fn dial<Msg>(min: i32, max: i32, value: i32) -> DialEl<Msg> {
    DialEl {
        min,
        max,
        value,
        on_change: None,
    }
}
impl<Msg> DialEl<Msg> {
    pub fn on_change(mut self, f: impl Fn(i32) -> Msg + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for DialEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::Dial(self)
    }
}

/// An integer spin box. `on_change` maps the value to a message.
pub struct SpinBoxEl<Msg> {
    min: i32,
    max: i32,
    value: i32,
    on_change: Option<Rc<dyn Fn(i32) -> Msg>>,
}
pub fn spinbox<Msg>(min: i32, max: i32, value: i32) -> SpinBoxEl<Msg> {
    SpinBoxEl {
        min,
        max,
        value,
        on_change: None,
    }
}
impl<Msg> SpinBoxEl<Msg> {
    pub fn on_change(mut self, f: impl Fn(i32) -> Msg + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for SpinBoxEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::SpinBox(self)
    }
}

/// A floating-point spin box. `on_change` maps the value to a message.
pub struct DoubleSpinBoxEl<Msg> {
    min: f64,
    max: f64,
    value: f64,
    decimals: i32,
    step: f64,
    on_change: Option<Rc<dyn Fn(f64) -> Msg>>,
}
pub fn double_spinbox<Msg>(min: f64, max: f64, value: f64) -> DoubleSpinBoxEl<Msg> {
    DoubleSpinBoxEl {
        min,
        max,
        value,
        decimals: 2,
        step: 1.0,
        on_change: None,
    }
}
impl<Msg> DoubleSpinBoxEl<Msg> {
    /// Number of digits shown after the decimal point (default 2).
    pub fn decimals(mut self, n: i32) -> Self {
        self.decimals = n;
        self
    }
    /// Amount added/subtracted by the step buttons (default 1.0).
    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }
    pub fn on_change(mut self, f: impl Fn(f64) -> Msg + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for DoubleSpinBoxEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::DoubleSpinBox(self)
    }
}

/// A progress bar (display only).
pub struct ProgressBarEl {
    min: i32,
    max: i32,
    value: i32,
}
pub fn progress_bar(min: i32, max: i32, value: i32) -> ProgressBarEl {
    ProgressBarEl { min, max, value }
}
impl<Msg> IntoElement<Msg> for ProgressBarEl {
    fn into_element(self) -> Element<Msg> {
        Element::ProgressBar(self)
    }
}

/// A drop-down selection box. `on_change` maps the selected index to a message.
pub struct ComboBoxEl<Msg> {
    items: Vec<String>,
    current: i32,
    on_change: Option<Rc<dyn Fn(i32) -> Msg>>,
}
pub fn combo_box<Msg>() -> ComboBoxEl<Msg> {
    ComboBoxEl {
        items: Vec::new(),
        current: 0,
        on_change: None,
    }
}
impl<Msg> ComboBoxEl<Msg> {
    pub fn item(mut self, text: impl Into<String>) -> Self {
        self.items.push(text.into());
        self
    }
    pub fn items<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.items.extend(items.into_iter().map(Into::into));
        self
    }
    pub fn selected(mut self, index: i32) -> Self {
        self.current = index;
        self
    }
    pub fn on_change(mut self, f: impl Fn(i32) -> Msg + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for ComboBoxEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::ComboBox(self)
    }
}

/// A scrollable list of selectable rows (a playlist, a file list, …).
/// `on_select` maps the newly-highlighted row index to a message; `on_activate`
/// maps a double-clicked / Enter-activated row.
pub struct ListEl<Msg> {
    items: Vec<String>,
    current: i32,
    on_select: Option<Rc<dyn Fn(i32) -> Msg>>,
    on_activate: Option<Rc<dyn Fn(i32) -> Msg>>,
}
pub fn list<Msg>() -> ListEl<Msg> {
    ListEl {
        items: Vec::new(),
        current: -1,
        on_select: None,
        on_activate: None,
    }
}
impl<Msg> ListEl<Msg> {
    pub fn item(mut self, text: impl Into<String>) -> Self {
        self.items.push(text.into());
        self
    }
    pub fn items<I, S>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.items.extend(items.into_iter().map(Into::into));
        self
    }
    /// Sets the highlighted row (`-1` for none).
    pub fn selected(mut self, index: i32) -> Self {
        self.current = index;
        self
    }
    pub fn on_select(mut self, f: impl Fn(i32) -> Msg + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    /// Fires when a row is activated (double-click or Enter).
    pub fn on_activate(mut self, f: impl Fn(i32) -> Msg + 'static) -> Self {
        self.on_activate = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for ListEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::List(self)
    }
}

/// A thin dividing line. Use [`separator`] for a horizontal rule (in a column)
/// or [`separator_v`] for a vertical one (in a row). Display only.
pub struct SeparatorEl {
    vertical: bool,
}
/// A horizontal dividing line.
pub fn separator() -> SeparatorEl {
    SeparatorEl { vertical: false }
}
/// A vertical dividing line.
pub fn separator_v() -> SeparatorEl {
    SeparatorEl { vertical: true }
}
impl<Msg> IntoElement<Msg> for SeparatorEl {
    fn into_element(self) -> Element<Msg> {
        Element::Separator(self)
    }
}

/// A vertical ([`column`]) or horizontal ([`row`]) stack of children.
pub struct ContainerEl<Msg> {
    vertical: bool,
    spacing: Option<i32>,
    margin: Option<i32>,
    children: Vec<Element<Msg>>,
}
/// A vertical stack.
pub fn column<Msg>() -> ContainerEl<Msg> {
    ContainerEl {
        vertical: true,
        spacing: None,
        margin: None,
        children: Vec::new(),
    }
}
/// A horizontal stack.
pub fn row<Msg>() -> ContainerEl<Msg> {
    ContainerEl {
        vertical: false,
        ..column()
    }
}
impl<Msg> ContainerEl<Msg> {
    pub fn spacing(mut self, px: i32) -> Self {
        self.spacing = Some(px);
        self
    }
    pub fn padding(mut self, px: i32) -> Self {
        self.margin = Some(px);
        self
    }
    pub fn child(mut self, node: impl IntoElement<Msg>) -> Self {
        self.children.push(node.into_element());
        self
    }
    /// Adds several children from an iterator — handy for rendering a list.
    pub fn children<I, E>(mut self, nodes: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: IntoElement<Msg>,
    {
        self.children
            .extend(nodes.into_iter().map(IntoElement::into_element));
        self
    }
    pub fn stretch(mut self) -> Self {
        self.children.push(Element::Stretch);
        self
    }
}
impl<Msg> IntoElement<Msg> for ContainerEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::Container(self)
    }
}

/// A titled frame grouping related widgets. It stacks its children just like a
/// [`column`] (or a [`row`] if `.horizontal()`), inside a labelled box.
pub struct GroupBoxEl<Msg> {
    title: String,
    vertical: bool,
    spacing: Option<i32>,
    margin: Option<i32>,
    children: Vec<Element<Msg>>,
}
pub fn group_box<Msg>(title: impl Into<String>) -> GroupBoxEl<Msg> {
    GroupBoxEl {
        title: title.into(),
        vertical: true,
        spacing: None,
        margin: None,
        children: Vec::new(),
    }
}
impl<Msg> GroupBoxEl<Msg> {
    /// Lay the contents out horizontally instead of vertically.
    pub fn horizontal(mut self) -> Self {
        self.vertical = false;
        self
    }
    pub fn spacing(mut self, px: i32) -> Self {
        self.spacing = Some(px);
        self
    }
    pub fn padding(mut self, px: i32) -> Self {
        self.margin = Some(px);
        self
    }
    pub fn child(mut self, node: impl IntoElement<Msg>) -> Self {
        self.children.push(node.into_element());
        self
    }
    pub fn children<I, E>(mut self, nodes: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: IntoElement<Msg>,
    {
        self.children
            .extend(nodes.into_iter().map(IntoElement::into_element));
        self
    }
    pub fn stretch(mut self) -> Self {
        self.children.push(Element::Stretch);
        self
    }
}
impl<Msg> IntoElement<Msg> for GroupBoxEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::GroupBox(self)
    }
}

// ---- custom, user-defined widgets -----------------------------------------

/// An RGBA colour (0–255 per channel), used by [`Canvas`] drawing ops.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }
}

/// A mouse button, as reported on a [`MouseEvent`]. `None` appears on moves that
/// have no button held; `Other` covers extra buttons (back/forward/etc.).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    None,
    Left,
    Right,
    Middle,
    Other,
}
impl MouseButton {
    // Decodes the Qt button code the shim forwards. For moves the shim sends a
    // bitmask of held buttons; we surface the primary one (Left wins).
    fn from_code(code: i32) -> Self {
        match code {
            0 => MouseButton::None,
            _ if code & 0x1 != 0 => MouseButton::Left,   // Qt::LeftButton
            _ if code & 0x2 != 0 => MouseButton::Right,  // Qt::RightButton
            _ if code & 0x4 != 0 => MouseButton::Middle, // Qt::MiddleButton
            _ => MouseButton::Other,
        }
    }
}

/// A mouse event delivered to a [`custom`] widget's handlers. Coordinates are
/// widget-local pixels with the origin at the top-left — the same space
/// [`Canvas`] draws in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub x: i32,
    pub y: i32,
    /// On press/release, the button that changed. On move, the primary button
    /// held (or [`MouseButton::None`] when hovering with none down).
    pub button: MouseButton,
}

/// Raw mouse payload crossing the FFI boundary before it becomes a [`MouseEvent`].
#[derive(Clone, Copy)]
struct MouseRaw {
    kind: i32,
    x: i32,
    y: i32,
    button: i32,
}

/// A mouse-wheel event on a [`custom`] widget. `delta` is Qt's vertical
/// `angleDelta().y()`: positive scrolls up/away, one notch is typically ±120.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WheelEvent {
    pub x: i32,
    pub y: i32,
    pub delta: i32,
}

/// A safe drawing surface handed to [`CustomWidget::draw`]. It wraps the widget's
/// `QPainter` for the duration of one paint; every method is a safe wrapper over
/// the shim, so implementors never touch a raw pointer or an `unsafe` block.
/// Coordinates are in device-independent pixels with the origin at the top-left.
pub struct Canvas {
    p: *mut sys::QtPainter,
    width: i32,
    height: i32,
}
impl Canvas {
    /// Width of the paint area, in px.
    pub fn width(&self) -> i32 {
        self.width
    }
    /// Height of the paint area, in px.
    pub fn height(&self) -> i32 {
        self.height
    }
    /// `(width, height)` of the paint area.
    pub fn size(&self) -> (i32, i32) {
        (self.width, self.height)
    }

    /// Fills a rectangle.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        unsafe {
            sys::qt_painter_fill_rect(self.p, x, y, w, h, c.r as i32, c.g as i32, c.b as i32, c.a as i32)
        };
    }
    /// Fills the whole surface with a single colour.
    pub fn clear(&mut self, c: Color) {
        self.fill_rect(0, 0, self.width, self.height, c);
    }
    /// Strokes a rectangle outline with the given pen width.
    pub fn stroke_rect(&mut self, x: i32, y: i32, w: i32, h: i32, line: i32, c: Color) {
        unsafe {
            sys::qt_painter_stroke_rect(
                self.p, x, y, w, h, line, c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }
    /// Fills an ellipse inscribed in the given rectangle.
    pub fn fill_ellipse(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        unsafe {
            sys::qt_painter_fill_ellipse(
                self.p, x, y, w, h, c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }
    /// Draws a line with the given pen width.
    pub fn line(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, line: i32, c: Color) {
        unsafe {
            sys::qt_painter_draw_line(
                self.p, x1, y1, x2, y2, line, c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }
    /// Draws text with its baseline at `(x, y)`.
    pub fn text(&mut self, x: i32, y: i32, s: &str, c: Color) {
        let cs = cstr(s);
        unsafe {
            sys::qt_painter_draw_text(self.p, x, y, cs.as_ptr(), c.r as i32, c.g as i32, c.b as i32, c.a as i32)
        };
    }

    // ---- state, transforms, quality --------------------------------------

    /// Saves the painter state (transform, clip, opacity, font). Pair with
    /// [`Canvas::restore`]; nesting is fine.
    pub fn save(&mut self) {
        unsafe { sys::qt_painter_save(self.p) };
    }
    /// Restores the state saved by the matching [`Canvas::save`].
    pub fn restore(&mut self) {
        unsafe { sys::qt_painter_restore(self.p) };
    }
    /// Shifts the origin by `(dx, dy)` for subsequent drawing.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        unsafe { sys::qt_painter_translate(self.p, dx, dy) };
    }
    /// Rotates subsequent drawing clockwise by `degrees` about the origin.
    pub fn rotate(&mut self, degrees: f64) {
        unsafe { sys::qt_painter_rotate(self.p, degrees) };
    }
    /// Scales subsequent drawing by `(sx, sy)`.
    pub fn scale(&mut self, sx: f64, sy: f64) {
        unsafe { sys::qt_painter_scale(self.p, sx, sy) };
    }
    /// Sets the global opacity (0.0–1.0) applied to subsequent drawing.
    pub fn set_opacity(&mut self, opacity: f64) {
        unsafe { sys::qt_painter_set_opacity(self.p, opacity) };
    }
    /// Toggles antialiasing (and smooth image scaling) for subsequent drawing.
    pub fn set_antialiasing(&mut self, on: bool) {
        unsafe { sys::qt_painter_set_antialiasing(self.p, on as i32) };
    }
    /// Sets the font used by [`Canvas::text`]: family, pixel size, and weight.
    pub fn set_font(&mut self, family: &str, px: i32, bold: bool) {
        let f = cstr(family);
        unsafe { sys::qt_painter_set_font(self.p, f.as_ptr(), px, bold as i32) };
    }

    // ---- extra shapes ----------------------------------------------------

    /// Strokes an ellipse outline inscribed in the given rectangle.
    pub fn stroke_ellipse(&mut self, x: i32, y: i32, w: i32, h: i32, line: i32, c: Color) {
        unsafe {
            sys::qt_painter_stroke_ellipse(
                self.p, x, y, w, h, line, c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }
    /// Fills a rounded rectangle with corner radii `rx`/`ry`.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rounded_rect(&mut self, x: i32, y: i32, w: i32, h: i32, rx: f64, ry: f64, c: Color) {
        unsafe {
            sys::qt_painter_fill_rounded_rect(
                self.p, x, y, w, h, rx, ry, c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }
    /// Strokes a rounded rectangle outline with corner radii `rx`/`ry`.
    #[allow(clippy::too_many_arguments)]
    pub fn stroke_rounded_rect(
        &mut self, x: i32, y: i32, w: i32, h: i32, rx: f64, ry: f64, line: i32, c: Color,
    ) {
        unsafe {
            sys::qt_painter_stroke_rounded_rect(
                self.p, x, y, w, h, rx, ry, line, c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }
    /// Fills a closed polygon through the given points.
    pub fn fill_polygon(&mut self, points: &[(i32, i32)], c: Color) {
        let flat = flatten_points(points);
        unsafe {
            sys::qt_painter_fill_polygon(
                self.p, flat.as_ptr(), points.len() as i32,
                c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }
    /// Draws a connected series of line segments through the given points.
    pub fn polyline(&mut self, points: &[(i32, i32)], line: i32, c: Color) {
        let flat = flatten_points(points);
        unsafe {
            sys::qt_painter_draw_polyline(
                self.p, flat.as_ptr(), points.len() as i32, line,
                c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }

    // ---- gradients -------------------------------------------------------

    /// Fills a rectangle with a linear gradient from `(x1,y1)` colour `c1` to
    /// `(x2,y2)` colour `c2` (coordinates in the same space as the rect).
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_linear(
        &mut self, x: i32, y: i32, w: i32, h: i32,
        x1: f64, y1: f64, c1: Color, x2: f64, y2: f64, c2: Color,
    ) {
        unsafe {
            sys::qt_painter_fill_rect_lgrad(
                self.p, x, y, w, h, x1, y1, x2, y2,
                c1.r as i32, c1.g as i32, c1.b as i32, c1.a as i32,
                c2.r as i32, c2.g as i32, c2.b as i32, c2.a as i32,
            )
        };
    }
    /// Fills a rectangle with a radial gradient centred at `(cx,cy)` going from
    /// colour `inner` at the centre to `outer` at `radius`.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_radial(
        &mut self, x: i32, y: i32, w: i32, h: i32,
        cx: f64, cy: f64, radius: f64, inner: Color, outer: Color,
    ) {
        unsafe {
            sys::qt_painter_fill_rect_rgrad(
                self.p, x, y, w, h, cx, cy, radius,
                inner.r as i32, inner.g as i32, inner.b as i32, inner.a as i32,
                outer.r as i32, outer.g as i32, outer.b as i32, outer.a as i32,
            )
        };
    }

    // ---- paths -----------------------------------------------------------

    /// Fills a [`Path`].
    pub fn fill_path(&mut self, path: &Path, c: Color) {
        unsafe {
            sys::qt_painter_fill_path(self.p, path.0, c.r as i32, c.g as i32, c.b as i32, c.a as i32)
        };
    }
    /// Strokes a [`Path`] with the given pen width.
    pub fn stroke_path(&mut self, path: &Path, line: i32, c: Color) {
        unsafe {
            sys::qt_painter_stroke_path(
                self.p, path.0, line, c.r as i32, c.g as i32, c.b as i32, c.a as i32,
            )
        };
    }
    /// Clips subsequent drawing to a [`Path`]. Wrap in [`Canvas::save`] /
    /// [`Canvas::restore`] to undo the clip.
    pub fn clip_path(&mut self, path: &Path) {
        unsafe { sys::qt_painter_clip_path(self.p, path.0) };
    }

    // ---- images ----------------------------------------------------------

    /// Draws an [`Image`] with its top-left at `(x, y)`.
    pub fn image(&mut self, image: &Image, x: i32, y: i32) {
        unsafe { sys::qt_painter_draw_image(self.p, image.0, x, y) };
    }
    /// Draws an [`Image`] scaled to fill the given rectangle.
    pub fn image_scaled(&mut self, image: &Image, x: i32, y: i32, w: i32, h: i32) {
        unsafe { sys::qt_painter_draw_image_scaled(self.p, image.0, x, y, w, h) };
    }
}

/// Flattens `(x, y)` pairs into the interleaved int array the shim expects.
fn flatten_points(points: &[(i32, i32)]) -> Vec<i32> {
    let mut flat = Vec::with_capacity(points.len() * 2);
    for &(x, y) in points {
        flat.push(x);
        flat.push(y);
    }
    flat
}

/// A reusable vector path (lines and cubic Béziers) for [`Canvas::fill_path`],
/// [`Canvas::stroke_path`], and [`Canvas::clip_path`]. Build it with the moves
/// below; coordinates are in the canvas's pixel space.
pub struct Path(*mut sys::QtPath);

impl Path {
    /// Creates an empty path.
    pub fn new() -> Self {
        Path(unsafe { sys::qt_path_new() })
    }
    /// Starts a new sub-path at `(x, y)`.
    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        unsafe { sys::qt_path_move_to(self.0, x, y) };
        self
    }
    /// Adds a straight line to `(x, y)`.
    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        unsafe { sys::qt_path_line_to(self.0, x, y) };
        self
    }
    /// Adds a cubic Bézier to `(ex, ey)` with control points `c1`/`c2`.
    pub fn cubic_to(
        &mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, ex: f64, ey: f64,
    ) -> &mut Self {
        unsafe { sys::qt_path_cubic_to(self.0, c1x, c1y, c2x, c2y, ex, ey) };
        self
    }
    /// Closes the current sub-path back to its start.
    pub fn close(&mut self) -> &mut Self {
        unsafe { sys::qt_path_close(self.0) };
        self
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Path {
    fn drop(&mut self) {
        unsafe { sys::qt_path_delete(self.0) };
    }
}

/// A decoded image (holds its pixels), created once and drawn many times via
/// [`Canvas::image`] / [`Canvas::image_scaled`]. Keep it in your widget's state
/// rather than reloading each frame.
pub struct Image(*mut sys::QtImage);

impl Image {
    /// Loads an image from a file path (PNG, JPEG, …). Returns `None` on failure.
    pub fn load(path: &str) -> Option<Self> {
        let p = cstr(path);
        let ptr = unsafe { sys::qt_image_load(p.as_ptr()) };
        (!ptr.is_null()).then_some(Image(ptr))
    }
    /// Decodes an image from encoded bytes in memory. Returns `None` on failure.
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        let ptr = unsafe { sys::qt_image_from_data(data.as_ptr(), data.len() as i32) };
        (!ptr.is_null()).then_some(Image(ptr))
    }
    /// `(width, height)` in pixels.
    pub fn size(&self) -> (i32, i32) {
        unsafe { (sys::qt_image_width(self.0), sys::qt_image_height(self.0)) }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe { sys::qt_image_delete(self.0) };
    }
}

/// A custom-drawn widget: everything the built-in set doesn't cover — a spectrum
/// `visualizer`, an `audio_display`, any bespoke rendering — expressed by
/// painting into a safe [`Canvas`]. No raw pointers, no `unsafe`.
///
/// ```ignore
/// struct Visualizer { bars: Vec<f32> }
/// impl CustomWidget for Visualizer {
///     fn draw(&self, cx: &mut Canvas) {
///         cx.clear(Color::BLACK);
///         let (w, h) = cx.size();
///         let bw = w / self.bars.len().max(1) as i32;
///         for (i, &v) in self.bars.iter().enumerate() {
///             let bh = (v * h as f32) as i32;
///             cx.fill_rect(i as i32 * bw, h - bh, bw - 2, bh, Color::rgb(80, 200, 120));
///         }
///     }
/// }
/// fn visualizer<Msg>(bars: &[f32]) -> impl IntoElement<Msg> {
///     custom(Visualizer { bars: bars.to_vec() })
/// }
/// // ...in view:
/// row().child(visualizer(&self.spectrum))
/// ```
///
/// The diff treats each concrete implementor type as its own widget kind: the
/// same type across renders keeps the underlying widget alive and just requests
/// a repaint with the new data; a different type rebuilds. (For components that
/// are only a composition of existing widgets, you don't need this at all — a
/// plain `fn(..) -> impl IntoElement<Msg>` returning `column()`/`row()` does it.)
pub trait CustomWidget: 'static {
    /// Paints the widget. Called by Qt whenever the area needs redrawing; the
    /// runtime schedules a repaint after every diff that keeps this widget, so
    /// `draw` always sees the latest state.
    fn draw(&self, canvas: &mut Canvas);
    /// Optional fixed size, in px. Return `Some((w, h))` to pin the widget's
    /// size; `None` lets the layout decide.
    fn size(&self) -> Option<(i32, i32)> {
        None
    }
}

/// A mouse-event handler mapping a [`MouseEvent`] to a message. Shared (`Rc`) so
/// it survives being cloned into diff closures.
type MouseHandler<Msg> = Rc<dyn Fn(MouseEvent) -> Msg>;

/// The element produced by [`custom`].
pub struct CustomEl<Msg> {
    type_id: TypeId,
    inner: Box<dyn CustomWidget>,
    on_down: Option<MouseHandler<Msg>>,
    on_up: Option<MouseHandler<Msg>>,
    on_move: Option<MouseHandler<Msg>>,
    on_wheel: Option<Rc<dyn Fn(WheelEvent) -> Msg>>,
    on_resize: Option<Rc<dyn Fn(i32, i32) -> Msg>>,
    hover: bool,
    _msg: PhantomData<fn() -> Msg>,
}

/// Wraps a [`CustomWidget`] into an [`Element`] for use in a `view`.
pub fn custom<Msg, W: CustomWidget>(widget: W) -> CustomEl<Msg> {
    CustomEl {
        type_id: TypeId::of::<W>(),
        inner: Box::new(widget),
        on_down: None,
        on_up: None,
        on_move: None,
        on_wheel: None,
        on_resize: None,
        hover: false,
        _msg: PhantomData,
    }
}
impl<Msg> CustomEl<Msg> {
    /// Handles mouse-button presses over the widget.
    pub fn on_mouse_down(mut self, f: impl Fn(MouseEvent) -> Msg + 'static) -> Self {
        self.on_down = Some(Rc::new(f));
        self
    }
    /// Handles mouse-button releases over the widget.
    pub fn on_mouse_up(mut self, f: impl Fn(MouseEvent) -> Msg + 'static) -> Self {
        self.on_up = Some(Rc::new(f));
        self
    }
    /// Handles mouse movement. By default moves fire only while a button is held
    /// (a drag); call [`CustomEl::hover`] to also receive moves with no button
    /// down. On a move event, `MouseEvent::button` reports the buttons held.
    pub fn on_mouse_move(mut self, f: impl Fn(MouseEvent) -> Msg + 'static) -> Self {
        self.on_move = Some(Rc::new(f));
        self
    }
    /// Enables hover tracking, so [`CustomEl::on_mouse_move`] also fires while no
    /// button is held. Off by default (moves come only during a drag).
    pub fn hover(mut self) -> Self {
        self.hover = true;
        self
    }
    /// Handles mouse-wheel scrolls over the widget (e.g. a volume knob).
    pub fn on_wheel(mut self, f: impl Fn(WheelEvent) -> Msg + 'static) -> Self {
        self.on_wheel = Some(Rc::new(f));
        self
    }
    /// Handles the widget being resized, receiving the new `(width, height)`.
    pub fn on_resize(mut self, f: impl Fn(i32, i32) -> Msg + 'static) -> Self {
        self.on_resize = Some(Rc::new(f));
        self
    }
}
impl<Msg> IntoElement<Msg> for CustomEl<Msg> {
    fn into_element(self) -> Element<Msg> {
        Element::Custom(self)
    }
}

// ===========================================================================
// Mounted tree (what is actually on screen, diffed against new Elements)
// ===========================================================================

/// A live, realized node: the Qt pointer plus a copy of the props last applied,
/// so the diff can tell what changed. Message-carrying widgets keep the raw
/// pointer to their [`Slot`] (owned by the runtime's `sinks`).
enum Mounted {
    Label {
        w: *mut sys::QtWidget,
        text: String,
    },
    Button {
        w: *mut sys::QtWidget,
        text: String,
        slot: *const Slot<()>,
    },
    Checkbox {
        w: *mut sys::QtWidget,
        text: String,
        checked: bool,
        slot: *const Slot<bool>,
    },
    RadioButton {
        w: *mut sys::QtWidget,
        text: String,
        checked: bool,
        slot: *const Slot<bool>,
    },
    LineEdit {
        w: *mut sys::QtWidget,
        text: String,
        placeholder: Option<String>,
        slot: *const Slot<String>,
    },
    TextEdit {
        w: *mut sys::QtWidget,
        text: String,
        placeholder: Option<String>,
        read_only: bool,
        slot: *const Slot<String>,
    },
    Slider {
        w: *mut sys::QtWidget,
        value: i32,
        slot: *const Slot<i32>,
    },
    Dial {
        w: *mut sys::QtWidget,
        value: i32,
        slot: *const Slot<i32>,
    },
    SpinBox {
        w: *mut sys::QtWidget,
        value: i32,
        slot: *const Slot<i32>,
    },
    DoubleSpinBox {
        w: *mut sys::QtWidget,
        value: f64,
        slot: *const Slot<f64>,
    },
    ProgressBar {
        w: *mut sys::QtWidget,
        value: i32,
    },
    ComboBox {
        w: *mut sys::QtWidget,
        items: Vec<String>,
        current: i32,
        slot: *const Slot<i32>,
    },
    List {
        w: *mut sys::QtWidget,
        items: Vec<String>,
        current: i32,
        select_slot: *const Slot<i32>,
        activate_slot: *const Slot<i32>,
    },
    Separator {
        w: *mut sys::QtWidget,
    },
    Container {
        layout: *mut sys::QtLayout,
        vertical: bool,
        children: Vec<Mounted>,
    },
    GroupBox {
        w: *mut sys::QtWidget,
        layout: *mut sys::QtLayout,
        title: String,
        vertical: bool,
        children: Vec<Mounted>,
    },
    Custom {
        w: *mut sys::QtWidget,
        type_id: TypeId,
        slot: *const CanvasSlot,
        /// The fixed size last applied to the canvas, so the diff can re-run the
        /// widget's `size()` hint each render and only touch Qt when it changes.
        size: Option<(i32, i32)>,
        /// The mouse-event slot; its closure is re-pointed each render so the
        /// handlers always produce the current state's messages.
        mouse_slot: *const Slot<MouseRaw>,
        /// The wheel- and resize-event slots, re-pointed each render like `mouse_slot`.
        wheel_slot: *const Slot<WheelEvent>,
        resize_slot: *const Slot<(i32, i32)>,
        /// Whether hover tracking is currently on (a move handler is present),
        /// so the diff only toggles Qt when it changes.
        tracking: bool,
    },
    Stretch,
}

/// How a mounted node attaches to a parent box layout.
enum Attach {
    Widget(*mut sys::QtWidget),
    Layout(*mut sys::QtLayout),
    Stretch,
}

impl Mounted {
    fn attach(&self) -> Attach {
        match self {
            Mounted::Label { w, .. }
            | Mounted::Button { w, .. }
            | Mounted::Checkbox { w, .. }
            | Mounted::RadioButton { w, .. }
            | Mounted::LineEdit { w, .. }
            | Mounted::TextEdit { w, .. }
            | Mounted::Slider { w, .. }
            | Mounted::Dial { w, .. }
            | Mounted::SpinBox { w, .. }
            | Mounted::DoubleSpinBox { w, .. }
            | Mounted::ProgressBar { w, .. }
            | Mounted::ComboBox { w, .. }
            | Mounted::List { w, .. }
            | Mounted::Separator { w, .. }
            | Mounted::GroupBox { w, .. }
            | Mounted::Custom { w, .. } => Attach::Widget(*w),
            Mounted::Container { layout, .. } => Attach::Layout(*layout),
            Mounted::Stretch => Attach::Stretch,
        }
    }
}

/// Inserts a realized node into `layout` at `index`.
fn insert_at(layout: *mut sys::QtLayout, index: i32, node: &Mounted) {
    unsafe {
        match node.attach() {
            Attach::Widget(w) => sys::qt_layout_insert_widget(layout, index, w),
            Attach::Layout(l) => sys::qt_layout_insert_layout(layout, index, l),
            Attach::Stretch => sys::qt_layout_insert_stretch(layout, index),
        }
    }
}

/// Does a live node and a fresh element describe the same kind of widget? If not
/// the diff replaces the node wholesale instead of patching it. Container
/// orientation is part of the identity: a `column` never morphs into a `row`.
fn same_kind<Msg>(m: &Mounted, e: &Element<Msg>) -> bool {
    matches!(
        (m, e),
        (Mounted::Label { .. }, Element::Label(_))
            | (Mounted::Button { .. }, Element::Button(_))
            | (Mounted::Checkbox { .. }, Element::Checkbox(_))
            | (Mounted::RadioButton { .. }, Element::RadioButton(_))
            | (Mounted::LineEdit { .. }, Element::LineEdit(_))
            | (Mounted::TextEdit { .. }, Element::TextEdit(_))
            | (Mounted::Slider { .. }, Element::Slider(_))
            | (Mounted::Dial { .. }, Element::Dial(_))
            | (Mounted::SpinBox { .. }, Element::SpinBox(_))
            | (Mounted::DoubleSpinBox { .. }, Element::DoubleSpinBox(_))
            | (Mounted::ProgressBar { .. }, Element::ProgressBar(_))
            | (Mounted::ComboBox { .. }, Element::ComboBox(_))
            | (Mounted::List { .. }, Element::List(_))
            | (Mounted::Separator { .. }, Element::Separator(_))
            | (Mounted::Stretch, Element::Stretch)
    ) || matches!(
        (m, e),
        (Mounted::Container { vertical, .. }, Element::Container(c)) if *vertical == c.vertical
    ) || matches!(
        (m, e),
        (Mounted::GroupBox { vertical, .. }, Element::GroupBox(c)) if *vertical == c.vertical
    ) || matches!(
        (m, e),
        (Mounted::Custom { type_id, .. }, Element::Custom(c)) if *type_id == c.type_id
    )
}

// ===========================================================================
// Build context: owns the slots so their raw pointers stay valid
// ===========================================================================

/// Threaded through realize/patch. Retains every event slot for the runtime's
/// lifetime (so a widget removed by a diff can never dangle a live Qt signal),
/// and carries the [`Dispatch`] handlers close over.
struct Ctx<Msg> {
    d: Dispatch<Msg>,
    sinks: Rc<RefCell<Vec<Box<dyn Any>>>>,
}

impl<Msg: Clone + 'static> Ctx<Msg> {
    /// Boxes a slot, keeps it alive in `sinks`, and returns a stable pointer Qt
    /// (and later the diff) can hold onto.
    fn keep<A: 'static>(&self, f: Box<dyn Fn(A)>) -> *const Slot<A> {
        let slot = Box::new(Slot(RefCell::new(f)));
        let ptr: *const Slot<A> = &*slot;
        self.sinks.borrow_mut().push(slot);
        ptr
    }

    /// Retains a custom-widget slot for the runtime's lifetime (so a removed
    /// canvas can never fire paint into freed state) and returns a stable pointer.
    fn keep_canvas(&self, widget: Box<dyn CustomWidget>) -> *const CanvasSlot {
        let slot = Box::new(CanvasSlot(RefCell::new(widget)));
        let ptr: *const CanvasSlot = &*slot;
        self.sinks.borrow_mut().push(slot);
        ptr
    }

    fn click(&self, on: Option<Msg>) -> Box<dyn Fn(())> {
        let d = self.d.clone();
        Box::new(move |()| {
            if let Some(m) = &on {
                d.emit(m.clone());
            }
        })
    }
    fn map_i32(&self, map: Option<Rc<dyn Fn(i32) -> Msg>>) -> Box<dyn Fn(i32)> {
        let d = self.d.clone();
        Box::new(move |v| {
            if let Some(f) = &map {
                d.emit(f(v));
            }
        })
    }
    /// Builds a canvas mouse handler that routes a raw event to the matching
    /// per-kind handler (down/move/up) and queues the resulting message.
    fn mouse(
        &self,
        down: Option<MouseHandler<Msg>>,
        up: Option<MouseHandler<Msg>>,
        mv: Option<MouseHandler<Msg>>,
    ) -> Box<dyn Fn(MouseRaw)> {
        let d = self.d.clone();
        Box::new(move |m| {
            let handler = match m.kind {
                0 => &down,
                1 => &mv,
                2 => &up,
                _ => &None,
            };
            if let Some(f) = handler {
                let ev = MouseEvent {
                    x: m.x,
                    y: m.y,
                    button: MouseButton::from_code(m.button),
                };
                d.emit(f(ev));
            }
        })
    }

    /// Builds a canvas wheel handler that queues the mapped message (or nothing).
    fn wheel(&self, h: Option<Rc<dyn Fn(WheelEvent) -> Msg>>) -> Box<dyn Fn(WheelEvent)> {
        let d = self.d.clone();
        Box::new(move |e| {
            if let Some(f) = &h {
                d.emit(f(e));
            }
        })
    }

    /// Builds a canvas resize handler that queues the mapped message (or nothing).
    fn resize(&self, h: Option<Rc<dyn Fn(i32, i32) -> Msg>>) -> Box<dyn Fn((i32, i32))> {
        let d = self.d.clone();
        Box::new(move |(w, ht)| {
            if let Some(f) = &h {
                d.emit(f(w, ht));
            }
        })
    }

    /// A timer tick handler: each fire computes a fresh message and queues it.
    fn tick(&self, make: Rc<dyn Fn() -> Msg>) -> Box<dyn Fn(())> {
        let d = self.d.clone();
        Box::new(move |()| d.emit(make()))
    }
    fn map_f64(&self, map: Option<Rc<dyn Fn(f64) -> Msg>>) -> Box<dyn Fn(f64)> {
        let d = self.d.clone();
        Box::new(move |v| {
            if let Some(f) = &map {
                d.emit(f(v));
            }
        })
    }
    fn map_bool(&self, map: Option<Rc<dyn Fn(bool) -> Msg>>) -> Box<dyn Fn(bool)> {
        let d = self.d.clone();
        Box::new(move |v| {
            if let Some(f) = &map {
                d.emit(f(v));
            }
        })
    }
    fn map_str(&self, map: Option<StrHandler<Msg>>) -> Box<dyn Fn(String)> {
        let d = self.d.clone();
        Box::new(move |s: String| {
            if let Some(f) = &map {
                d.emit(f(&s));
            }
        })
    }
}

/// Overwrites a live slot's closure (used when a diff keeps a widget but its
/// handler may now produce a different message).
fn set_slot<A>(slot: *const Slot<A>, f: Box<dyn Fn(A)>) {
    unsafe { *(*slot).0.borrow_mut() = f };
}

// ===========================================================================
// Realize: Element -> Mounted (build fresh widgets)
// ===========================================================================

fn realize<Msg: Clone + 'static>(el: Element<Msg>, ctx: &Ctx<Msg>) -> Mounted {
    match el {
        Element::Label(e) => {
            let w = unsafe { sys::qt_label_new(cstr(&e.text).as_ptr()) };
            Mounted::Label { w, text: e.text }
        }
        Element::Button(e) => {
            let w = unsafe { sys::qt_button_new(cstr(&e.text).as_ptr()) };
            let slot = ctx.keep(ctx.click(e.on_click));
            unsafe { sys::qt_button_on_clicked(w, tramp_void, slot as *mut c_void) };
            Mounted::Button { w, text: e.text, slot }
        }
        Element::Checkbox(e) => {
            let w = unsafe { sys::qt_checkbox_new(cstr(&e.text).as_ptr()) };
            unsafe { sys::qt_checkbox_set_checked(w, e.checked as i32) };
            let slot = ctx.keep(ctx.map_bool(e.on_toggle));
            unsafe { sys::qt_checkbox_on_toggled(w, tramp_bool, slot as *mut c_void) };
            Mounted::Checkbox {
                w,
                text: e.text,
                checked: e.checked,
                slot,
            }
        }
        Element::RadioButton(e) => {
            let w = unsafe { sys::qt_radio_button_new(cstr(&e.text).as_ptr()) };
            unsafe { sys::qt_radio_button_set_checked(w, e.checked as i32) };
            let slot = ctx.keep(ctx.map_bool(e.on_toggle));
            unsafe { sys::qt_radio_button_on_toggled(w, tramp_bool, slot as *mut c_void) };
            Mounted::RadioButton {
                w,
                text: e.text,
                checked: e.checked,
                slot,
            }
        }
        Element::LineEdit(e) => {
            let w = unsafe { sys::qt_line_edit_new(cstr(&e.text).as_ptr()) };
            if let Some(p) = &e.placeholder {
                unsafe { sys::qt_line_edit_set_placeholder(w, cstr(p).as_ptr()) };
            }
            let slot = ctx.keep(ctx.map_str(e.on_change));
            unsafe { sys::qt_line_edit_on_changed(w, tramp_str, slot as *mut c_void) };
            Mounted::LineEdit {
                w,
                text: e.text,
                placeholder: e.placeholder,
                slot,
            }
        }
        Element::TextEdit(e) => {
            let w = unsafe { sys::qt_text_edit_new(cstr(&e.text).as_ptr()) };
            if let Some(p) = &e.placeholder {
                unsafe { sys::qt_text_edit_set_placeholder(w, cstr(p).as_ptr()) };
            }
            if e.read_only {
                unsafe { sys::qt_text_edit_set_read_only(w, 1) };
            }
            let slot = ctx.keep(ctx.map_str(e.on_change));
            unsafe { sys::qt_text_edit_on_changed(w, tramp_str, slot as *mut c_void) };
            Mounted::TextEdit {
                w,
                text: e.text,
                placeholder: e.placeholder,
                read_only: e.read_only,
                slot,
            }
        }
        Element::Slider(e) => {
            let w = unsafe { sys::qt_slider_new(e.min, e.max, e.value) };
            let slot = ctx.keep(ctx.map_i32(e.on_change));
            unsafe { sys::qt_slider_on_changed(w, tramp_int, slot as *mut c_void) };
            Mounted::Slider {
                w,
                value: e.value,
                slot,
            }
        }
        Element::Dial(e) => {
            let w = unsafe { sys::qt_dial_new(e.min, e.max, e.value) };
            let slot = ctx.keep(ctx.map_i32(e.on_change));
            unsafe { sys::qt_dial_on_changed(w, tramp_int, slot as *mut c_void) };
            Mounted::Dial {
                w,
                value: e.value,
                slot,
            }
        }
        Element::SpinBox(e) => {
            let w = unsafe { sys::qt_spinbox_new(e.min, e.max, e.value) };
            let slot = ctx.keep(ctx.map_i32(e.on_change));
            unsafe { sys::qt_spinbox_on_changed(w, tramp_int, slot as *mut c_void) };
            Mounted::SpinBox {
                w,
                value: e.value,
                slot,
            }
        }
        Element::DoubleSpinBox(e) => {
            let w =
                unsafe { sys::qt_double_spinbox_new(e.min, e.max, e.value, e.decimals, e.step) };
            let slot = ctx.keep(ctx.map_f64(e.on_change));
            unsafe { sys::qt_double_spinbox_on_changed(w, tramp_double, slot as *mut c_void) };
            Mounted::DoubleSpinBox {
                w,
                value: e.value,
                slot,
            }
        }
        Element::ProgressBar(e) => {
            let w = unsafe { sys::qt_progress_bar_new(e.min, e.max, e.value) };
            Mounted::ProgressBar { w, value: e.value }
        }
        Element::ComboBox(e) => {
            let w = unsafe { sys::qt_combo_box_new() };
            for it in &e.items {
                unsafe { sys::qt_combo_box_add_item(w, cstr(it).as_ptr()) };
            }
            unsafe { sys::qt_combo_box_set_current_index(w, e.current) };
            let slot = ctx.keep(ctx.map_i32(e.on_change));
            unsafe { sys::qt_combo_box_on_changed(w, tramp_int, slot as *mut c_void) };
            Mounted::ComboBox {
                w,
                items: e.items,
                current: e.current,
                slot,
            }
        }
        Element::List(e) => {
            let w = unsafe { sys::qt_list_new() };
            for it in &e.items {
                unsafe { sys::qt_list_add_item(w, cstr(it).as_ptr()) };
            }
            unsafe { sys::qt_list_set_current_row(w, e.current) };
            let select_slot = ctx.keep(ctx.map_i32(e.on_select));
            let activate_slot = ctx.keep(ctx.map_i32(e.on_activate));
            unsafe {
                sys::qt_list_on_current_changed(w, tramp_int, select_slot as *mut c_void);
                sys::qt_list_on_activated(w, tramp_int, activate_slot as *mut c_void);
            };
            Mounted::List {
                w,
                items: e.items,
                current: e.current,
                select_slot,
                activate_slot,
            }
        }
        Element::Separator(e) => {
            let w = unsafe { sys::qt_separator_new(e.vertical as i32) };
            Mounted::Separator { w }
        }
        Element::Container(e) => realize_container(e, ctx),
        Element::GroupBox(e) => realize_group_box(e, ctx),
        Element::Custom(e) => {
            let size = e.inner.size();
            let slot = ctx.keep_canvas(e.inner);
            let w = unsafe { sys::qt_canvas_new(tramp_paint, slot as *mut c_void) };
            apply_canvas_size(w, None, size);
            // Always attach the input slots so handlers can appear on a later
            // render without rebuilding; tracking is opt-in via `.hover()`.
            let tracking = e.hover;
            let mouse_slot = ctx.keep(ctx.mouse(e.on_down, e.on_up, e.on_move));
            let wheel_slot = ctx.keep(ctx.wheel(e.on_wheel));
            let resize_slot = ctx.keep(ctx.resize(e.on_resize));
            unsafe {
                sys::qt_canvas_on_mouse(w, tramp_mouse, mouse_slot as *mut c_void, tracking as i32);
                sys::qt_canvas_on_wheel(w, tramp_wheel, wheel_slot as *mut c_void);
                sys::qt_canvas_on_resize(w, tramp_resize, resize_slot as *mut c_void);
            };
            unsafe { sys::qt_widget_update(w) };
            Mounted::Custom {
                w,
                type_id: e.type_id,
                slot,
                size,
                mouse_slot,
                wheel_slot,
                resize_slot,
                tracking,
            }
        }
        Element::Stretch => Mounted::Stretch,
    }
}

/// Applies a custom widget's size hint to its canvas, doing nothing when it is
/// unchanged from `old`. `None` releases any previously pinned size back to the
/// layout.
fn apply_canvas_size(w: *mut sys::QtWidget, old: Option<(i32, i32)>, new: Option<(i32, i32)>) {
    if old == new {
        return;
    }
    match new {
        Some((cw, ch)) => unsafe { sys::qt_widget_set_fixed_size(w, cw, ch) },
        None => unsafe { sys::qt_widget_unset_fixed_size(w) },
    }
}

fn realize_container<Msg: Clone + 'static>(e: ContainerEl<Msg>, ctx: &Ctx<Msg>) -> Mounted {
    let layout = unsafe { sys::qt_box_layout_new(e.vertical as i32) };
    if let Some(s) = e.spacing {
        unsafe { sys::qt_layout_set_spacing(layout, s) };
    }
    if let Some(m) = e.margin {
        unsafe { sys::qt_layout_set_margins(layout, m, m, m, m) };
    }
    let mut children = Vec::with_capacity(e.children.len());
    for child in e.children {
        let node = realize(child, ctx);
        match node.attach() {
            Attach::Widget(w) => unsafe { sys::qt_layout_add_widget(layout, w) },
            Attach::Layout(l) => unsafe { sys::qt_layout_add_layout(layout, l) },
            Attach::Stretch => unsafe { sys::qt_layout_add_stretch(layout) },
        }
        children.push(node);
    }
    Mounted::Container {
        layout,
        vertical: e.vertical,
        children,
    }
}

fn realize_group_box<Msg: Clone + 'static>(e: GroupBoxEl<Msg>, ctx: &Ctx<Msg>) -> Mounted {
    let w = unsafe { sys::qt_group_box_new(cstr(&e.title).as_ptr()) };
    let layout = unsafe { sys::qt_box_layout_new(e.vertical as i32) };
    if let Some(s) = e.spacing {
        unsafe { sys::qt_layout_set_spacing(layout, s) };
    }
    if let Some(m) = e.margin {
        unsafe { sys::qt_layout_set_margins(layout, m, m, m, m) };
    }
    let mut children = Vec::with_capacity(e.children.len());
    for child in e.children {
        let node = realize(child, ctx);
        match node.attach() {
            Attach::Widget(cw) => unsafe { sys::qt_layout_add_widget(layout, cw) },
            Attach::Layout(l) => unsafe { sys::qt_layout_add_layout(layout, l) },
            Attach::Stretch => unsafe { sys::qt_layout_add_stretch(layout) },
        }
        children.push(node);
    }
    unsafe { sys::qt_widget_set_layout(w, layout) };
    Mounted::GroupBox {
        w,
        layout,
        title: e.title,
        vertical: e.vertical,
        children,
    }
}

// ===========================================================================
// Patch: reconcile a Mounted node with a fresh Element in place
// ===========================================================================

/// Updates `m` to match `el`, assuming [`same_kind`] already held. Only touches
/// Qt for props that actually differ.
fn patch<Msg: Clone + 'static>(m: &mut Mounted, el: Element<Msg>, ctx: &Ctx<Msg>) {
    match (m, el) {
        (Mounted::Label { w, text }, Element::Label(e)) => {
            if *text != e.text {
                unsafe { sys::qt_label_set_text(*w, cstr(&e.text).as_ptr()) };
                *text = e.text;
            }
        }
        (Mounted::Button { w, text, slot }, Element::Button(e)) => {
            if *text != e.text {
                unsafe { sys::qt_button_set_text(*w, cstr(&e.text).as_ptr()) };
                *text = e.text;
            }
            set_slot(*slot, ctx.click(e.on_click));
        }
        (
            Mounted::Checkbox {
                w,
                text,
                checked,
                slot,
            },
            Element::Checkbox(e),
        ) => {
            if *text != e.text {
                unsafe { sys::qt_checkbox_set_text(*w, cstr(&e.text).as_ptr()) };
                *text = e.text;
            }
            if *checked != e.checked {
                let w = *w;
                quietly(w, || unsafe {
                    sys::qt_checkbox_set_checked(w, e.checked as i32)
                });
                *checked = e.checked;
            }
            set_slot(*slot, ctx.map_bool(e.on_toggle));
        }
        (
            Mounted::RadioButton {
                w,
                text,
                checked,
                slot,
            },
            Element::RadioButton(e),
        ) => {
            if *text != e.text {
                unsafe { sys::qt_radio_button_set_text(*w, cstr(&e.text).as_ptr()) };
                *text = e.text;
            }
            if *checked != e.checked {
                let w = *w;
                quietly(w, || unsafe {
                    sys::qt_radio_button_set_checked(w, e.checked as i32)
                });
                *checked = e.checked;
            }
            set_slot(*slot, ctx.map_bool(e.on_toggle));
        }
        (
            Mounted::LineEdit {
                w,
                text,
                placeholder,
                slot,
            },
            Element::LineEdit(e),
        ) => {
            if *text != e.text {
                let w = *w;
                quietly(w, || unsafe {
                    sys::qt_line_edit_set_text(w, cstr(&e.text).as_ptr())
                });
                *text = e.text;
            }
            if *placeholder != e.placeholder {
                let p = e.placeholder.clone().unwrap_or_default();
                unsafe { sys::qt_line_edit_set_placeholder(*w, cstr(&p).as_ptr()) };
                *placeholder = e.placeholder;
            }
            set_slot(*slot, ctx.map_str(e.on_change));
        }
        (
            Mounted::TextEdit {
                w,
                text,
                placeholder,
                read_only,
                slot,
            },
            Element::TextEdit(e),
        ) => {
            if *text != e.text {
                let w = *w;
                quietly(w, || unsafe {
                    sys::qt_text_edit_set_text(w, cstr(&e.text).as_ptr())
                });
                *text = e.text;
            }
            if *placeholder != e.placeholder {
                let p = e.placeholder.clone().unwrap_or_default();
                unsafe { sys::qt_text_edit_set_placeholder(*w, cstr(&p).as_ptr()) };
                *placeholder = e.placeholder;
            }
            if *read_only != e.read_only {
                unsafe { sys::qt_text_edit_set_read_only(*w, e.read_only as i32) };
                *read_only = e.read_only;
            }
            set_slot(*slot, ctx.map_str(e.on_change));
        }
        (Mounted::Slider { w, value, slot }, Element::Slider(e)) => {
            if *value != e.value {
                let w = *w;
                quietly(w, || unsafe { sys::qt_slider_set_value(w, e.value) });
                *value = e.value;
            }
            set_slot(*slot, ctx.map_i32(e.on_change));
        }
        (Mounted::Dial { w, value, slot }, Element::Dial(e)) => {
            if *value != e.value {
                let w = *w;
                quietly(w, || unsafe { sys::qt_dial_set_value(w, e.value) });
                *value = e.value;
            }
            set_slot(*slot, ctx.map_i32(e.on_change));
        }
        (Mounted::SpinBox { w, value, slot }, Element::SpinBox(e)) => {
            if *value != e.value {
                let w = *w;
                quietly(w, || unsafe { sys::qt_spinbox_set_value(w, e.value) });
                *value = e.value;
            }
            set_slot(*slot, ctx.map_i32(e.on_change));
        }
        (Mounted::DoubleSpinBox { w, value, slot }, Element::DoubleSpinBox(e)) => {
            if *value != e.value {
                let w = *w;
                quietly(w, || unsafe { sys::qt_double_spinbox_set_value(w, e.value) });
                *value = e.value;
            }
            set_slot(*slot, ctx.map_f64(e.on_change));
        }
        (Mounted::ProgressBar { w, value }, Element::ProgressBar(e)) => {
            if *value != e.value {
                unsafe { sys::qt_progress_bar_set_value(*w, e.value) };
                *value = e.value;
            }
        }
        (
            Mounted::ComboBox {
                w,
                items,
                current,
                slot,
            },
            Element::ComboBox(e),
        ) => {
            if *items != e.items {
                let w = *w;
                quietly(w, || unsafe {
                    sys::qt_combo_box_clear(w);
                    for it in &e.items {
                        sys::qt_combo_box_add_item(w, cstr(it).as_ptr());
                    }
                });
                *items = e.items;
                *current = -1; // force the index set below
            }
            if *current != e.current {
                let w = *w;
                quietly(w, || unsafe { sys::qt_combo_box_set_current_index(w, e.current) });
                *current = e.current;
            }
            set_slot(*slot, ctx.map_i32(e.on_change));
        }
        (
            Mounted::List {
                w,
                items,
                current,
                select_slot,
                activate_slot,
            },
            Element::List(e),
        ) => {
            if *items != e.items {
                let w = *w;
                quietly(w, || unsafe {
                    sys::qt_list_clear(w);
                    for it in &e.items {
                        sys::qt_list_add_item(w, cstr(it).as_ptr());
                    }
                });
                *items = e.items;
                *current = -2; // force the row set below (clear reset it to -1)
            }
            if *current != e.current {
                let w = *w;
                quietly(w, || unsafe { sys::qt_list_set_current_row(w, e.current) });
                *current = e.current;
            }
            set_slot(*select_slot, ctx.map_i32(e.on_select));
            set_slot(*activate_slot, ctx.map_i32(e.on_activate));
        }
        (Mounted::Separator { .. }, Element::Separator(_)) => {}
        (Mounted::Container { layout, children, .. }, Element::Container(e)) => {
            diff_children(*layout, children, e.children, ctx);
        }
        (
            Mounted::GroupBox {
                w,
                layout,
                title,
                children,
                ..
            },
            Element::GroupBox(e),
        ) => {
            if *title != e.title {
                unsafe { sys::qt_group_box_set_title(*w, cstr(&e.title).as_ptr()) };
                *title = e.title;
            }
            diff_children(*layout, children, e.children, ctx);
        }
        (
            Mounted::Custom {
                w,
                slot,
                size,
                mouse_slot,
                wheel_slot,
                resize_slot,
                tracking,
                ..
            },
            Element::Custom(e),
        ) => {
            // Re-read the size hint every render: it may depend on state that
            // changed. Only touch Qt when the preferred size actually differs.
            let new_size = e.inner.size();
            apply_canvas_size(*w, *size, new_size);
            *size = new_size;
            // Re-point the input handlers so they emit the latest messages, and
            // toggle hover tracking only if the `.hover()` flag changed.
            let new_tracking = e.hover;
            set_slot(*mouse_slot, ctx.mouse(e.on_down, e.on_up, e.on_move));
            set_slot(*wheel_slot, ctx.wheel(e.on_wheel));
            set_slot(*resize_slot, ctx.resize(e.on_resize));
            if *tracking != new_tracking {
                unsafe { sys::qt_canvas_set_mouse_tracking(*w, new_tracking as i32) };
                *tracking = new_tracking;
            }
            // Swap the new props in behind the same canvas, then repaint.
            *unsafe { &*(*slot) }.0.borrow_mut() = e.inner;
            unsafe { sys::qt_widget_update(*w) };
        }
        (Mounted::Stretch, Element::Stretch) => {}
        // same_kind guarantees the arms above are exhaustive for real pairs.
        _ => unreachable!("patch called on mismatched node kinds"),
    }
}

/// Reconciles a container's children by position: patch in place where the kind
/// matches, replace where it diverges, then append or trim the tail.
fn diff_children<Msg: Clone + 'static>(
    layout: *mut sys::QtLayout,
    old: &mut Vec<Mounted>,
    new: Vec<Element<Msg>>,
    ctx: &Ctx<Msg>,
) {
    let mut new = new.into_iter();
    let mut i = 0;
    while i < old.len() {
        match new.next() {
            Some(e) => {
                if same_kind(&old[i], &e) {
                    patch(&mut old[i], e, ctx);
                } else {
                    unsafe { sys::qt_layout_remove_at(layout, i as i32) };
                    let node = realize(e, ctx);
                    insert_at(layout, i as i32, &node);
                    old[i] = node;
                }
                i += 1;
            }
            None => break,
        }
    }
    // New tree is shorter: drop the surplus tail (highest index first).
    while old.len() > i {
        unsafe { sys::qt_layout_remove_at(layout, (old.len() - 1) as i32) };
        old.pop();
    }
    // New tree is longer: append the remaining fresh nodes.
    for e in new {
        let node = realize(e, ctx);
        let idx = old.len() as i32;
        insert_at(layout, idx, &node);
        old.push(node);
    }
}

// ===========================================================================
// Timer subscriptions: reconcile live QTimers against the declared list
// ===========================================================================

/// A live timer. Owns its callback slot behind a stable box so the raw pointer
/// handed to Qt survives the `Vec` moving, and deletes the QTimer on drop (which
/// also stops it), so removing a subscription tears the timer down cleanly.
struct MountedTimer {
    timer: *mut sys::QtTimer,
    interval_ms: u64,
    slot: Box<Slot<()>>,
}

impl MountedTimer {
    fn realize<Msg: Clone + 'static>(s: Subscription<Msg>, ctx: &Ctx<Msg>) -> Self {
        let slot = Box::new(Slot(RefCell::new(ctx.tick(s.make))));
        let ptr: *const Slot<()> = &*slot;
        let timer =
            unsafe { sys::qt_timer_new(s.interval_ms as i32, tramp_void, ptr as *mut c_void) };
        MountedTimer {
            timer,
            interval_ms: s.interval_ms,
            slot,
        }
    }
}

impl Drop for MountedTimer {
    fn drop(&mut self) {
        // Delete (and stop) the QTimer before its slot box drops, so no queued
        // tick can fire into freed state.
        unsafe { sys::qt_timer_delete(self.timer) };
    }
}

/// Reconciles the running timers against a fresh subscription list, positionally
/// (mirroring [`diff_children`]): keep and re-point matching timers, drop the
/// surplus tail, create fresh timers for any additions.
fn diff_timers<Msg: Clone + 'static>(
    old: &mut Vec<MountedTimer>,
    subs: Vec<Subscription<Msg>>,
    ctx: &Ctx<Msg>,
) {
    let mut subs = subs.into_iter();
    let mut i = 0;
    while i < old.len() {
        match subs.next() {
            Some(s) => {
                if old[i].interval_ms != s.interval_ms {
                    unsafe { sys::qt_timer_set_interval(old[i].timer, s.interval_ms as i32) };
                    old[i].interval_ms = s.interval_ms;
                }
                // Re-point the tick handler so it emits the latest message.
                let slot: *const Slot<()> = &*old[i].slot;
                set_slot(slot, ctx.tick(s.make));
                i += 1;
            }
            None => break,
        }
    }
    // Fewer timers now: drop the surplus (their Drop stops + deletes them).
    old.truncate(i);
    // More timers now: start the fresh ones.
    for s in subs {
        old.push(MountedTimer::realize(s, ctx));
    }
}

// ===========================================================================
// Async: cross-thread message emitter
// ===========================================================================

/// A thread-safe, cloneable handle for feeding messages into the UI from *other*
/// threads — a background download, a decode worker, a `std::thread`, or a task
/// on any async runtime. The reactive runtime itself is single-threaded (it lives
/// on the GUI thread); an `Emitter` is the one piece that crosses the boundary.
///
/// Get one from [`Ui::emitter`] (after `mount`). Messages are queued and applied
/// on the GUI thread on its next event-loop turn, exactly like a widget event, so
/// your `update`/`view` never run off-thread. `Msg` must be [`Send`].
///
/// ```no_run
/// # use qax::ui::*; use qax::Application;
/// # #[derive(Clone)] enum Msg { Done(String) }
/// # struct App; impl Component for App {
/// #   type Message = Msg;
/// #   fn update(&mut self, _m: Msg) {}
/// #   fn view(&self) -> Element<Msg> { label("x").into_element() }
/// # }
/// # let app = Application::new();
/// let ui = Ui::new(App).mount();
/// let tx = ui.emitter();
/// std::thread::spawn(move || {
///     let data = std::fs::read_to_string("/etc/hostname").unwrap();
///     tx.emit(Msg::Done(data)); // wakes the UI thread safely
/// });
/// # app.exec();
/// ```
pub struct Emitter<Msg> {
    inbox: Arc<Mutex<VecDeque<Msg>>>,
    scheduled: Arc<AtomicBool>,
    /// Address of the leaked `Box<dyn Fn()>` drain closure (main-thread only). We
    /// carry it as a `usize` so `Emitter` stays auto-`Send`; it is dereferenced
    /// solely on the GUI thread inside [`tramp_flush`].
    poke: usize,
}

impl<Msg> Clone for Emitter<Msg> {
    fn clone(&self) -> Self {
        Emitter {
            inbox: self.inbox.clone(),
            scheduled: self.scheduled.clone(),
            poke: self.poke,
        }
    }
}

impl<Msg: Send + 'static> Emitter<Msg> {
    /// Queues `msg` and wakes the GUI thread to apply it. Safe to call from any
    /// thread. Coalesces: many rapid emits collapse into one re-render turn.
    pub fn emit(&self, msg: Msg) {
        self.inbox.lock().unwrap().push_back(msg);
        // Only poke the GUI thread if a drain is not already pending.
        if !self.scheduled.swap(true, Ordering::AcqRel) {
            unsafe { sys::qt_post_to_main(tramp_flush, self.poke as *mut c_void) };
        }
    }
}

// ===========================================================================
// Menu bar
// ===========================================================================

/// One entry in a [`Menu`]: an action that emits a message, or a separator.
enum MenuItem<Msg> {
    Action { text: String, msg: Msg },
    Separator,
}

/// A top-level menu for the window's menu bar, built with [`menu`] and attached
/// with [`Ui::menu`]. Selecting an action emits its message like any widget event.
///
/// ```ignore
/// Ui::new(app)
///     .menu(menu("File").action("Open…", Msg::Open).separator().action("Quit", Msg::Quit))
///     .mount();
/// ```
pub struct Menu<Msg> {
    title: String,
    items: Vec<MenuItem<Msg>>,
}
/// Starts a menu with the given title (use `&Text` mnemonics if you like).
pub fn menu<Msg>(title: impl Into<String>) -> Menu<Msg> {
    Menu {
        title: title.into(),
        items: Vec::new(),
    }
}
impl<Msg> Menu<Msg> {
    /// Adds an action row that emits `msg` when chosen.
    pub fn action(mut self, text: impl Into<String>, msg: Msg) -> Self {
        self.items.push(MenuItem::Action {
            text: text.into(),
            msg,
        });
        self
    }
    /// Adds a separator line.
    pub fn separator(mut self) -> Self {
        self.items.push(MenuItem::Separator);
        self
    }
}

/// Builds a menu's actions into the given native `QtMenu`, wiring each action to
/// emit its message through `ctx`.
fn realize_menu<Msg: Clone + 'static>(native: *mut sys::QtMenu, m: Menu<Msg>, ctx: &Ctx<Msg>) {
    for item in m.items {
        match item {
            MenuItem::Action { text, msg } => {
                let slot = ctx.keep(ctx.click(Some(msg)));
                unsafe {
                    sys::qt_menu_add_action(native, cstr(&text).as_ptr(), tramp_void, slot as *mut c_void)
                };
            }
            MenuItem::Separator => unsafe { sys::qt_menu_add_separator(native) },
        }
    }
}

// ===========================================================================
// Runtime + Ui handle
// ===========================================================================

struct Runtime<C: Component> {
    comp: C,
    /// Always a `Mounted::Container` (the implicit root); its `children[0]` is
    /// the tree the user's `view` produced.
    root: Mounted,
    /// Live timers declared by `subscriptions`, diffed alongside the view.
    timers: Vec<MountedTimer>,
    ctx: Ctx<C::Message>,
}

impl<C: Component> Runtime<C> {
    fn rerender(&mut self) {
        let view = self.comp.view();
        if let Mounted::Container { children, layout, .. } = &mut self.root {
            // Reconcile the single logical child against the new view.
            let new = vec![view];
            diff_children(*layout, children, new, &self.ctx);
        }
        // Then reconcile timers against the current state's subscriptions.
        let subs = self.comp.subscriptions();
        diff_timers(&mut self.timers, subs, &self.ctx);
    }
}

/// A mounted, live UI. Keep it in scope until the event loop returns — dropping
/// it releases every retained event slot. Create one with [`Ui::new`].
pub struct Ui<C: Component> {
    rt: Rc<RefCell<Runtime<C>>>,
    /// The top-level `QMainWindow`.
    window: *mut sys::QtWidget,
    /// The central widget hosting the reactive layout (menus/status live on the
    /// window around it).
    central: *mut sys::QtWidget,
    // Owns the deferred-flush closure Qt posts back to; freed on drop.
    flush: *mut Box<dyn Fn()>,
    /// Cross-thread inbox and its coalescing flag, shared with every [`Emitter`].
    inbox: Arc<Mutex<VecDeque<C::Message>>>,
    cross_scheduled: Arc<AtomicBool>,
    /// The leaked drain closure emitters poke via `qt_post_to_main`. Intentionally
    /// never reclaimed, so a late emit from a detached thread can't dangle.
    cross_flush: *mut Box<dyn Fn()>,
    title: Option<String>,
    size: Option<(i32, i32)>,
    menus: Vec<Menu<C::Message>>,
    status: Option<String>,
}

impl<C: Component> Ui<C> {
    /// Wraps a component. Nothing is shown until [`Ui::mount`].
    pub fn new(component: C) -> Self {
        Ui {
            rt: Rc::new(RefCell::new(Runtime {
                comp: component,
                root: Mounted::Stretch, // placeholder, replaced in mount()
                timers: Vec::new(),
                ctx: Ctx {
                    // Filled in by mount(); this Dispatch is never used before then.
                    d: Dispatch {
                        queue: Rc::new(RefCell::new(VecDeque::new())),
                        scheduled: Rc::new(Cell::new(false)),
                        flush: std::ptr::null(),
                    },
                    sinks: Rc::new(RefCell::new(Vec::new())),
                },
            })),
            window: unsafe { sys::qt_main_window_new() },
            central: unsafe { sys::qt_widget_new() },
            flush: std::ptr::null_mut(),
            inbox: Arc::new(Mutex::new(VecDeque::new())),
            cross_scheduled: Arc::new(AtomicBool::new(false)),
            cross_flush: std::ptr::null_mut(),
            title: None,
            size: None,
            menus: Vec::new(),
            status: None,
        }
    }

    /// Injects a message as if it came from a widget event, applying it and
    /// re-rendering synchronously. Handy for driving the UI programmatically or
    /// from tests, without waiting on the event loop. No-op before [`Ui::mount`].
    pub fn dispatch(&self, msg: C::Message) {
        self.rt.borrow_mut().comp.update(msg);
        self.rt.borrow_mut().rerender();
    }

    /// Reads from the current component state without mutating it. Runs `f` with
    /// a shared borrow of the component and returns its result — handy for tests
    /// or for pulling a value out to hand to non-reactive code.
    pub fn state<R>(&self, f: impl FnOnce(&C) -> R) -> R {
        f(&self.rt.borrow().comp)
    }

    /// Returns a thread-safe [`Emitter`] for feeding messages in from background
    /// threads or async tasks. Call after [`Ui::mount`]. Requires the message
    /// type to be [`Send`].
    pub fn emitter(&self) -> Emitter<C::Message>
    where
        C::Message: Send,
    {
        debug_assert!(
            !self.cross_flush.is_null(),
            "Ui::emitter() must be called after mount()"
        );
        Emitter {
            inbox: self.inbox.clone(),
            scheduled: self.cross_scheduled.clone(),
            poke: self.cross_flush as usize,
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
    pub fn size(mut self, width: i32, height: i32) -> Self {
        self.size = Some((width, height));
        self
    }
    /// Adds a top-level [`Menu`] to the window's menu bar. Call once per menu, in
    /// order; each action emits its message like any widget event.
    pub fn menu(mut self, menu: Menu<C::Message>) -> Self {
        self.menus.push(menu);
        self
    }
    /// Sets the initial status-bar text shown at the bottom of the window.
    pub fn status(mut self, text: impl Into<String>) -> Self {
        self.status = Some(text.into());
        self
    }

    /// Builds the initial widget tree from `view`, shows the window, and wires
    /// up reactive updates. Returns the handle to keep alive during `exec`.
    pub fn mount(mut self) -> Self {
        // Build the deferred flush: drain queued messages, apply them, re-render.
        let weak: Weak<RefCell<Runtime<C>>> = Rc::downgrade(&self.rt);
        let (queue, scheduled) = {
            let rt = self.rt.borrow();
            (rt.ctx.d.queue.clone(), rt.ctx.d.scheduled.clone())
        };
        // Re-entrancy guard shared by both flushes. A `Component::update` handler
        // may open a modal dialog (`dialog::input`, etc.), which spins a nested Qt
        // event loop; that loop can deliver another posted flush while we still
        // hold `rt.borrow_mut()`. Bailing out avoids a double borrow — the queued
        // messages are picked up by the outer drain loop once the dialog returns.
        let in_flush = Rc::new(Cell::new(false));
        let flush: Box<dyn Fn()> = Box::new({
            let queue = queue.clone();
            let scheduled = scheduled.clone();
            let in_flush = in_flush.clone();
            move || {
                if in_flush.replace(true) {
                    return;
                }
                let Some(rt) = weak.upgrade() else {
                    in_flush.set(false);
                    return;
                };
                // Outer loop: a re-entrant flush (posted while a modal dialog spun
                // a nested event loop) bailed above, leaving `scheduled` set. Keep
                // draining until it stays clear so future `emit`s post a fresh
                // flush instead of assuming one is already pending.
                loop {
                    scheduled.set(false);
                    loop {
                        let msg = queue.borrow_mut().pop_front();
                        let Some(msg) = msg else { break };
                        rt.borrow_mut().comp.update(msg);
                    }
                    rt.borrow_mut().rerender();
                    if !scheduled.get() {
                        break;
                    }
                }
                in_flush.set(false);
            }
        });
        // Leak-stable pointer Qt posts back to; reclaimed in Drop.
        let flush_ptr = Box::into_raw(Box::new(flush));
        self.flush = flush_ptr;

        // Cross-thread drain: applies messages that Emitters queued from other
        // threads. Runs only on the GUI thread (poked via qt_post_to_main). Leaked
        // deliberately — a detached thread may emit after the Ui is dropped, and
        // this closure only no-ops (its Weak fails to upgrade) rather than dangle.
        let cross_flush: Box<dyn Fn()> = Box::new({
            let weak: Weak<RefCell<Runtime<C>>> = Rc::downgrade(&self.rt);
            let inbox = self.inbox.clone();
            let scheduled = self.cross_scheduled.clone();
            let in_flush = in_flush.clone();
            move || {
                if in_flush.replace(true) {
                    return;
                }
                let Some(rt) = weak.upgrade() else {
                    scheduled.store(false, Ordering::Release);
                    in_flush.set(false);
                    return;
                };
                // Same re-entrancy discipline as the main flush: loop until the
                // schedule flag stays clear so a message queued during a modal
                // dialog isn't stranded with no flush pending.
                loop {
                    scheduled.store(false, Ordering::Release);
                    loop {
                        let msg = inbox.lock().unwrap().pop_front();
                        let Some(msg) = msg else { break };
                        rt.borrow_mut().comp.update(msg);
                    }
                    rt.borrow_mut().rerender();
                    if !scheduled.load(Ordering::Acquire) {
                        break;
                    }
                }
                in_flush.set(false);
            }
        });
        self.cross_flush = Box::into_raw(Box::new(cross_flush));

        // Now the Dispatch handlers will actually use.
        let dispatch = Dispatch {
            queue,
            scheduled,
            flush: flush_ptr as *const c_void,
        };

        // Realize the initial tree under an implicit root container and install
        // it as the window's layout.
        let root = {
            let mut rt = self.rt.borrow_mut();
            rt.ctx.d = dispatch;
            let view = rt.comp.view();
            let root_el: ContainerEl<C::Message> = ContainerEl {
                vertical: true,
                spacing: None,
                margin: None,
                children: vec![view],
            };
            realize_container(root_el, &rt.ctx)
        };
        if let Mounted::Container { layout, .. } = &root {
            // Layout goes on the central widget; the QMainWindow hosts it plus
            // the menu bar and status bar around it.
            unsafe { sys::qt_widget_set_layout(self.central, *layout) };
        }
        unsafe { sys::qt_main_window_set_central(self.window, self.central) };
        self.rt.borrow_mut().root = root;

        // Build the menu bar (actions dispatch through the same runtime).
        for m in std::mem::take(&mut self.menus) {
            let rt = self.rt.borrow();
            let native = unsafe { sys::qt_main_window_add_menu(self.window, cstr(&m.title).as_ptr()) };
            realize_menu(native, m, &rt.ctx);
        }

        // Start any timers the initial state subscribes to.
        {
            let mut rt = self.rt.borrow_mut();
            let subs = rt.comp.subscriptions();
            let Runtime { timers, ctx, .. } = &mut *rt;
            diff_timers(timers, subs, ctx);
        }

        if let Some(t) = &self.title {
            unsafe { sys::qt_widget_set_window_title(self.window, cstr(t).as_ptr()) };
        }
        if let Some(s) = &self.status {
            unsafe { sys::qt_main_window_set_status(self.window, cstr(s).as_ptr()) };
        }
        if let Some((w, h)) = self.size {
            unsafe { sys::qt_widget_resize(self.window, w, h) };
        }
        unsafe { sys::qt_widget_show(self.window) };
        self
    }
}

impl<C: Component> Drop for Ui<C> {
    fn drop(&mut self) {
        // Delete the top-level window; Qt's parent hierarchy takes the whole
        // child widget/layout tree down with it. Then reclaim the flush box that
        // outlived the posted callbacks.
        if !self.window.is_null() {
            unsafe { sys::qt_widget_delete(self.window) };
            self.window = std::ptr::null_mut();
        }
        if !self.flush.is_null() {
            unsafe { drop(Box::from_raw(self.flush)) };
            self.flush = std::ptr::null_mut();
        }
    }
}
