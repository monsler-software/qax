//! Regression test for the event-slot leak: a widget removed by the diff must
//! free the Rust state it owns (its event-handler slot / custom-widget box)
//! there and then, not retain it for the whole runtime lifetime. A churning UI
//! that repeatedly adds and removes widgets would otherwise grow without bound.
//!
//! We can't measure heap bytes portably, so we use a custom widget carrying a
//! `Drop` counter as a proxy: its box lives in the mounted node's slot, so it is
//! dropped exactly when that node is torn down. Runs headless (offscreen).

use std::sync::atomic::{AtomicUsize, Ordering};

use qax::ui::*;
use qax::Application;

/// Number of `Marker` widgets torn down. Each removed-from-the-tree custom widget
/// must drop as it is removed; if removed nodes were retained forever this would
/// stay at zero until the whole `Ui` is dropped.
static DROPS: AtomicUsize = AtomicUsize::new(0);

struct Marker;
impl CustomWidget for Marker {
    fn draw(&self, _cx: &mut Canvas) {}
    fn size(&self) -> Option<(i32, i32)> {
        Some((10, 10))
    }
}
impl Drop for Marker {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone)]
enum Msg {
    SetCount(usize),
}

#[derive(Default)]
struct State {
    count: usize,
}

impl Component for State {
    type Message = Msg;
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::SetCount(n) => self.count = n,
        }
    }
    fn view(&self) -> Element<Msg> {
        // A row of `count` custom widgets. Shrinking the count removes the tail
        // widgets, which must drop their boxed state on removal.
        column()
            .child(row().children((0..self.count).map(|_| custom::<Msg, _>(Marker))))
            .into_element()
    }
}

#[test]
fn removed_widgets_free_their_slots() {
    unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    let _app = Application::new();

    let ui = Ui::new(State::default()).title("churn").size(100, 100).mount();

    // Repeatedly grow to 50 widgets then shrink back to 0. Every removal must
    // drop its Marker; after many cycles the drop count reflects everything that
    // was ever removed, proving removed nodes aren't retained for the runtime's
    // lifetime.
    let cycles = 20;
    for _ in 0..cycles {
        ui.dispatch(Msg::SetCount(50));
        ui.dispatch(Msg::SetCount(0));
    }

    // Each cycle creates 50 and removes 50, so all 20*50 must have dropped by now
    // — while at most a handful of live widgets could remain (here: zero).
    let dropped = DROPS.load(Ordering::SeqCst);
    assert_eq!(
        dropped,
        cycles * 50,
        "every widget removed by the diff must free its slot on removal, not leak"
    );

    drop(ui);
}
