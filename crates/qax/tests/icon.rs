//! Exercises the shared [`Icon`] source across every widget that carries one:
//! buttons, checkboxes, radio buttons, combo-box and list items, menu actions
//! and the window itself. Both a path/resource icon and a desktop-theme icon
//! (with a path fallback) are set, then swapped through the diff so the icon
//! reconciliation arms run. Runs headless (offscreen) so it needs no display or
//! an actual icon theme — a missing theme icon just falls back or draws nothing.

use qax::ui::*;
use qax::{Application, Icon};

#[derive(Clone)]
enum Msg {
    Swap,
}

#[derive(Default)]
struct State {
    swapped: bool,
}

impl Component for State {
    type Message = Msg;
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Swap => self.swapped = !self.swapped,
        }
    }
    fn view(&self) -> Element<Msg> {
        // Flip each widget's icon between a theme name and a plain path so the
        // diff's icon arms fire (and clearing back to none is covered too).
        let (btn_icon, none_icon): (Icon, Option<Icon>) = if self.swapped {
            (Icon::theme_or("document-open", ":/open.png"), None)
        } else {
            (Icon::path(":/open.png"), Some(Icon::theme("edit-copy")))
        };

        let mut combo = combo_box::<Msg>()
            .item_icon("Open", Icon::theme("document-open"))
            .item("Plain");
        let mut list = list::<Msg>()
            .item_icon("Row", ":/open.png")
            .item("Plain");
        if self.swapped {
            combo = combo.item_icon("Extra", Icon::path(":/extra.png"));
            list = list.item("Extra");
        }

        let mut check = checkbox::<Msg>("Check");
        let mut radio = radio_button::<Msg>("Radio");
        if let Some(i) = none_icon {
            check = check.icon(i.clone());
            radio = radio.icon(i);
        }

        column()
            .child(button("Open").icon(btn_icon))
            .child(check)
            .child(radio)
            .child(combo)
            .child(list)
            .into_element()
    }
}

#[test]
fn icons_across_widgets() {
    let app = Application::new();
    let ui = Ui::new(State::default())
        // A theme window icon with a resource fallback, plus a `&str` path form.
        .icon(Icon::theme_or("applications-graphics", ":/app.png"))
        .menu(
            menu("File")
                .action_icon("Open…", Icon::theme("document-open"), Msg::Swap)
                .action("Quit", Msg::Swap),
        )
        .mount();

    // Runtime path form still works.
    ui.set_icon(":/app.png");

    // Swap every icon and back, driving both diff directions.
    ui.dispatch(Msg::Swap);
    assert!(ui.state(|s| s.swapped));
    ui.dispatch(Msg::Swap);
    assert!(ui.state(|s| !s.swapped));

    drop(ui);
    drop(app);
}
