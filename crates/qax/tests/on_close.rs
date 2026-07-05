//! Verifies `Ui::on_close`: the handler fires when the window is closed (here via
//! `Ui::close`), exactly once, and reclaiming the `Ui` afterwards is clean. This
//! is the hook a multi-window app uses to drop a closed window's handle so closed
//! windows don't accumulate. Runs headless (offscreen).

use std::cell::Cell;
use std::rc::Rc;

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Noop,
}

#[derive(Default)]
struct State;

impl Component for State {
    type Message = Msg;
    fn update(&mut self, _msg: Msg) {}
    fn view(&self) -> Element<Msg> {
        column().child(button("x").on_click(Msg::Noop)).into_element()
    }
}

#[test]
fn on_close_fires_once() {
    unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    let app = Application::new();

    let closes = Rc::new(Cell::new(0));
    let ui = {
        let closes = closes.clone();
        Ui::new(State)
            .title("closable")
            .on_close(move || closes.set(closes.get() + 1))
            .mount()
    };

    assert_eq!(closes.get(), 0, "handler must not fire before close");

    // Programmatic close delivers a close event; the filter forwards it.
    ui.close();
    app.run_for(30);
    assert_eq!(closes.get(), 1, "close handler fires exactly once");

    // Dropping the handle after a close must not fire again nor double-free.
    drop(ui);
    assert_eq!(closes.get(), 1, "no spurious close on drop");
}
