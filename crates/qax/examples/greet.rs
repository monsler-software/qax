use qax::*;
use qax::ui::*;

struct Greeting {
    name: String,
    text_visible: bool
}

#[derive(Clone)]
enum Msg {
    SayHello(String),
    SwitchText,
    Exit
}

impl Component for Greeting {
    type Message = Msg;

    fn update(&mut self, msg: Self::Message) {
        match msg {
            Msg::SayHello(val) => {self.name = val; self.text_visible = false},
            Msg::SwitchText => {self.text_visible = !self.text_visible},
            Msg::Exit => {std::process::exit(0)}
        }
    }

    fn view(&self) -> Element<Self::Message> {
        let name = &self.name;

        column()
        .padding(8)
        .spacing(8)
        .child(
            line_edit()
            .placeholder("Enter your name")
            .on_change(|msg| Msg::SayHello(msg.to_owned())))
        .child(
            label(format!("Hello, {name}"))
            .visible(self.text_visible)
            .height(15))
        .child(row().child(
            button("Greet")
            .on_click(Msg::SwitchText))
            .child(button("")
            .icon(Icon::theme("application-exit"))
            .on_click(Msg::Exit).width(32)))
        .into_element()
    }
}


fn main() {
    let app = Application::new();

    let _ui = Ui::new(Greeting {name: String::new(), text_visible: false})
        .title("Hello, qax!")
        .size(300, 200)
        .centered()
        .mount();
    std::process::exit(app.exec());
}