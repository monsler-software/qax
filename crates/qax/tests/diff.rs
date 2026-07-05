//! Verifies the reactive runtime diffs instead of rebuilding: a state change
//! that only edits props must keep existing widgets, never recreate them. Runs
//! headless (offscreen) so it needs no display.

use std::sync::atomic::{AtomicUsize, Ordering};

use qax::ui::*;
use qax::Application;

/// Number of times a `Probe`'s `size()` hint was read. The runtime reads it once
/// per render (at mount and on every diff that keeps the widget), so it doubles
/// as a render counter for the probe without hooking runtime internals.
static SIZE_READS: AtomicUsize = AtomicUsize::new(0);

/// Number of times a `Probe` widget was actually torn down. A prop-only diff
/// reconciles in place and drops the old boxed widget as it swaps the new props
/// in; a *rebuild* would instead leak the old canvas, so this stays in lockstep
/// with the render count only while the widget is reused.
static DROPS: AtomicUsize = AtomicUsize::new(0);

impl Drop for Probe {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

/// A custom-drawn probe whose realize count we can observe.
struct Probe {
    value: f32,
}
impl CustomWidget for Probe {
    fn draw(&self, cx: &mut Canvas) {
        let (w, h) = cx.size();
        cx.fill_rect(0, 0, (self.value * w as f32) as i32, h, Color::WHITE);
    }
    fn size(&self) -> Option<(i32, i32)> {
        SIZE_READS.fetch_add(1, Ordering::SeqCst);
        Some((100, 20))
    }
}

#[derive(Clone)]
enum Msg {
    Inc,
    Push,
}

#[derive(Default)]
struct State {
    n: i64,
    items: Vec<i64>,
}

impl Component for State {
    type Message = Msg;
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Inc => self.n += 1,
            Msg::Push => self.items.push(self.items.len() as i64),
        }
    }
    fn view(&self) -> Element<Msg> {
        column()
            .child(custom::<Msg, _>(Probe {
                value: self.n as f32,
            }))
            .child(label(format!("count {}", self.n)))
            .child(row().children(self.items.iter().map(|i| label(format!("item {i}")))))
            .into_element()
    }
}

#[test]
fn diff_keeps_widgets_alive() {
    unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    let _app = Application::new();

    let ui = Ui::new(State::default()).title("t").size(100, 100).mount();
    assert_eq!(SIZE_READS.load(Ordering::SeqCst), 1, "size hint read at mount");
    assert_eq!(DROPS.load(Ordering::SeqCst), 0, "nothing torn down yet");

    // Prop-only changes reconcile the same custom widget in place: the size hint
    // is re-read every render (so it can track state), and exactly one old widget
    // box is dropped per render — never a rebuild that would leak a canvas.
    ui.dispatch(Msg::Inc);
    ui.dispatch(Msg::Inc);
    assert_eq!(
        SIZE_READS.load(Ordering::SeqCst),
        3,
        "size hint re-read on every render, not just at mount"
    );
    assert_eq!(
        DROPS.load(Ordering::SeqCst),
        2,
        "custom widget reconciled in place (one drop per render, no rebuild)"
    );

    // Growing a list inserts fresh children without disturbing the probe or
    // panicking during positional insertion.
    ui.dispatch(Msg::Push);
    ui.dispatch(Msg::Push);
    ui.dispatch(Msg::Push);
    assert_eq!(
        SIZE_READS.load(Ordering::SeqCst),
        6,
        "list growth still re-renders and reconciles the probe"
    );
    assert_eq!(
        DROPS.load(Ordering::SeqCst),
        5,
        "list growth leaves the probe reconciled in place, not rebuilt"
    );
}
