//! Smoke test for the window shell: a component with a menu bar, a status bar,
//! and a list widget mounts into a QMainWindow, renders, and reconciles a state
//! change (growing the list) without crashing. Runs headless (offscreen).

use qax::ui::*;
use qax::Application;

#[derive(Clone)]
enum Msg {
    Add,
    Select(i32),
    Quit,
}

#[derive(Default)]
struct State {
    items: Vec<String>,
    selected: i32,
}

impl Component for State {
    type Message = Msg;

    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Add => self.items.push(format!("track {}", self.items.len() + 1)),
            Msg::Select(i) => self.selected = i,
            Msg::Quit => {}
        }
    }

    fn view(&self) -> Element<Msg> {
        column()
            .child(
                list()
                    .items(self.items.clone())
                    .selected(self.selected)
                    .on_select(Msg::Select)
                    .on_activate(Msg::Select),
            )
            .child(label(format!("{} items", self.items.len())))
            .into_element()
    }
}

#[test]
fn window_shell_with_menu_and_list() {
    unsafe { std::env::set_var("QT_QPA_PLATFORM", "offscreen") };
    let app = Application::new();

    let ui = Ui::new(State::default())
        .title("shell")
        .size(240, 200)
        .status("ready")
        .menu(
            menu("File")
                .action("Add", Msg::Add)
                .separator()
                .action("Quit", Msg::Quit),
        )
        .menu(menu("Edit").action("Select first", Msg::Select(0)))
        .mount();

    app.run_for(20);

    // Grow the list across renders; the list widget reconciles in place.
    ui.dispatch(Msg::Add);
    ui.dispatch(Msg::Add);
    ui.dispatch(Msg::Add);
    app.run_for(20);

    assert_eq!(ui.state(|s| s.items.len()), 3, "list grew across renders");
}
