//! Covers the `.visible()` wrapper: toggling a component's visibility must
//! reconcile through the transparent style wrapper, keeping the child in the
//! tree (and its state) while only flipping whether it is shown. A hidden child
//! whose kind also changes must still take the wrapper's patch/rebuild paths
//! without tripping the `unreachable!` in `patch`. Runs headless (offscreen).

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Toggle,
    Swap,
}

#[derive(Default)]
struct State {
    shown: bool,
    swapped: bool,
}

impl Component for State {
    type Message = Msg;
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Toggle => self.shown = !self.shown,
            Msg::Swap => self.swapped = !self.swapped,
        }
    }
    fn view(&self) -> Element<Msg> {
        let child: Element<Msg> = if self.swapped {
            button("secret").on_click(Msg::Toggle).visible(self.shown)
        } else {
            label("secret").visible(self.shown)
        };
        column()
            .child(button("Toggle").on_click(Msg::Toggle))
            .child(child)
            .into_element()
    }
}

#[test]
fn visible_wrapper() {
    let app = Application::new();
    let ui = Ui::new(State::default()).mount();

    // Starts hidden; flip visibility repeatedly, patched through the wrapper.
    ui.dispatch(Msg::Toggle);
    assert!(ui.state(|s| s.shown));
    ui.dispatch(Msg::Toggle);
    assert!(ui.state(|s| !s.shown));

    // Flip the wrapped child's kind while hidden, then reveal it: exercises the
    // rebuild path under the wrapper together with the visibility flag.
    ui.dispatch(Msg::Swap);
    ui.dispatch(Msg::Toggle);
    assert!(ui.state(|s| s.shown && s.swapped));

    drop(ui);
    drop(app);
}
