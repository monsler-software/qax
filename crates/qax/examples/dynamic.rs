//! Dynamic list, state-driven: the set of widgets is derived from data, not
//! mutated by hand. "Add" pushes an item onto a `Vec` and the diff inserts the
//! one new button; "Clear" empties the `Vec` and the diff removes them all. No
//! container handles, no `add_child`/`clear` calls — just describe the list.
//!
//! Run with a display:  `cargo run -p qax --example dynamic`
use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Add,
    Clear,
    Clicked(usize),
}

#[derive(Default)]
struct State {
    items: Vec<usize>,
    last: Option<usize>,
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Add => self.items.push(self.items.len() + 1),
            Msg::Clear => {
                self.items.clear();
                self.last = None;
            }
            Msg::Clicked(n) => self.last = Some(n),
        }
    }

    fn view(&self) -> Element<Msg> {
        let status = match self.last {
            Some(n) => format!("{} items — last click: #{n}", self.items.len()),
            None => format!("{} items", self.items.len()),
        };

        let list = column().spacing(6).children(self.items.iter().map(|&n| {
            button(format!("Item #{n}")).on_click(Msg::Clicked(n))
        }));

        column()
            .spacing(12)
            .padding(16)
            .child(label(status))
            .child(
                row()
                    .spacing(8)
                    .child(button("Add").on_click(Msg::Add))
                    .child(button("Clear").on_click(Msg::Clear)),
            )
            .child(list)
            .stretch()
            .into_element()
    }
}

fn main() {
    let app = Application::new();
    let _ui = Ui::new(State::default())
        .title("qax — dynamic")
        .size(320, 400)
        .mount();
    std::process::exit(app.exec());
}
