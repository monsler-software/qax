//! Covers the `.stylesheet()`/`.tooltip()` wrapper and the full QPushButton.
//! A style-only change must reconcile through the transparent wrapper (which
//! would `unreachable!` in `patch` if `same_kind` and the patch arms disagreed),
//! and a checkable button must carry its toggle state through the diff. Runs
//! headless (offscreen) so it needs no display.

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Restyle,
    Toggle(bool),
    // A style wrapper whose child kind changes forces a rebuild path, not a patch.
    Swap,
}

#[derive(Default)]
struct State {
    accent: bool,
    on: bool,
    swapped: bool,
}

impl Component for State {
    type Message = Msg;
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Restyle => self.accent = !self.accent,
            Msg::Toggle(v) => self.on = v,
            Msg::Swap => self.swapped = !self.swapped,
        }
    }
    fn view(&self) -> Element<Msg> {
        let css = if self.accent {
            "background: #2d7;"
        } else {
            "background: #555;"
        };
        // A styled child whose kind flips between renders exercises the rebuild
        // branch of the diff underneath a wrapper.
        let styled_child: Element<Msg> = if self.swapped {
            label("swapped").stylesheet(css)
        } else {
            button("Mute")
                .checkable(true)
                .checked(self.on)
                .on_toggle(Msg::Toggle)
                .flat(true)
                .stylesheet(css)
                .tooltip("toggles mute")
        };
        column().child(styled_child).into_element()
    }
}

#[test]
fn styled_wrapper_and_button() {
    let app = Application::new();
    let ui = Ui::new(State::default()).mount();

    // Re-style repeatedly: patched in place through the transparent wrapper.
    ui.dispatch(Msg::Restyle);
    ui.dispatch(Msg::Restyle);
    ui.dispatch(Msg::Restyle);

    // The checkable button routes its toggle state through update().
    ui.dispatch(Msg::Toggle(true));
    assert!(ui.state(|s| s.on));

    // Flip the wrapped child's kind and back: forces rebuild, then re-realize.
    ui.dispatch(Msg::Swap);
    assert!(ui.state(|s| s.swapped));
    ui.dispatch(Msg::Swap);
    assert!(ui.state(|s| !s.swapped));

    drop(ui);
    drop(app);
}
