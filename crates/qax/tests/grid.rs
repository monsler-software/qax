//! Covers the `grid()` layout and the declarative `.width()`/`.height()` sizing.
//! A data-only change must patch grid cells in place (equal placement + kinds),
//! while adding/moving a cell must take the rebuild path — both of which would
//! `unreachable!` in `patch` if `same_kind` and the diff disagreed. Sizing is
//! toggled on and off so both the set and unset branches run. Runs headless.

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Bump,
    Grow,
    AddRow,
}

#[derive(Default)]
struct State {
    n: i64,
    wide: bool,
    rows: usize,
}

impl Component for State {
    type Message = Msg;
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Bump => self.n += 1,
            Msg::Grow => self.wide = !self.wide,
            Msg::AddRow => self.rows += 1,
        }
    }
    fn view(&self) -> Element<Msg> {
        let mut g = grid()
            .spacing(6)
            .padding(8)
            .cell(0, 0, label("Count:"))
            .cell(0, 1, label(format!("{}", self.n)))
            .span(1, 0, 1, 2, button("Bump").on_click(Msg::Bump));
        for r in 0..self.rows {
            g = g.cell(2 + r as i32, 0, label(format!("extra {r}")));
        }

        let sized = if self.wide {
            label("sized").width(200).height(40)
        } else {
            label("sized").height(40)
        };

        column().child(g).child(sized).into_element()
    }
}

#[test]
fn grid_and_sizing() {
    let app = Application::new();
    let ui = Ui::new(State::default()).mount();

    // Data-only updates: grid cells patched in place.
    ui.dispatch(Msg::Bump);
    ui.dispatch(Msg::Bump);
    assert_eq!(ui.state(|s| s.n), 2);

    // Toggle a fixed width on and off: exercises set + unset branches.
    ui.dispatch(Msg::Grow);
    assert!(ui.state(|s| s.wide));
    ui.dispatch(Msg::Grow);
    assert!(ui.state(|s| !s.wide));

    // Structural change: a new cell appears, forcing a grid rebuild.
    ui.dispatch(Msg::AddRow);
    ui.dispatch(Msg::AddRow);
    assert_eq!(ui.state(|s| s.rows), 2);

    // And keep working after the rebuild.
    ui.dispatch(Msg::Bump);
    assert_eq!(ui.state(|s| s.n), 3);

    drop(ui);
    drop(app);
}
