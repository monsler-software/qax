//! Regression test for the cross-thread drain leak. The runtime parks two
//! deferred-drain closures per window (a same-thread re-render drain and a
//! cross-thread inbox drain) in a small GUI-thread registry, keyed by a token an
//! `Emitter` can carry across threads by value. Dropping the `Ui` must remove
//! both — otherwise a multi-window app that opens and closes windows would grow
//! the registry without bound (the old design leaked the cross-thread one on
//! purpose). And a late emit after the `Ui` is gone must be a harmless no-op, not
//! a dangle. Runs headless (offscreen); one `Application` per process.

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Noop,
}

struct S;
impl Component for S {
    type Message = Msg;
    fn update(&mut self, _m: Msg) {}
    fn view(&self) -> Element<Msg> {
        column().child(button("x").on_click(Msg::Noop)).into_element()
    }
}

#[test]
fn drains_are_reclaimed_and_late_emit_is_safe() {
    unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    let app = Application::new();

    let base = qax::ui::__poke_registry_len();

    let ui = Ui::new(S).title("t").mount();
    assert_eq!(
        qax::ui::__poke_registry_len(),
        base + 2,
        "mount parks the main + cross drains"
    );

    let em = ui.emitter();
    drop(ui);
    assert_eq!(
        qax::ui::__poke_registry_len(),
        base,
        "dropping the Ui reclaims both drains — nothing leaked"
    );

    // The Ui (and its registry entries) are gone; a queued poke from this emitter
    // must find no entry and no-op rather than dangle into freed state.
    em.emit(Msg::Noop);
    app.run_for(20);

    // Repeated open/close cycles must not accumulate entries either.
    for _ in 0..25 {
        let ui = Ui::new(S).mount();
        drop(ui);
    }
    assert_eq!(
        qax::ui::__poke_registry_len(),
        base,
        "churning windows leaves the registry at its baseline"
    );
}
