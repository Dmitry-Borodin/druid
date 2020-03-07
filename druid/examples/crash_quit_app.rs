use druid::widget::{Button, Flex};
use druid::{AppLauncher, Widget, WindowDesc};

fn main () {
    let window = WindowDesc::new(build_widget);
    AppLauncher::with_window(window)
        .use_simple_logger()
        .launch(0_u32)
        .expect("launch failed");
}

fn build_widget() -> impl Widget<u32> {
    Flex::column()
        .with_child(
            Button::new(
                "Quit",
                |evt_ctx , data: _, _env| {
                    dbg!(&data);
                    let command = druid::Command::new(druid::commands::QUIT_APP, evt_ctx.window_id());
                    evt_ctx.submit_command(command, None);
                },
            ),
            1.0,
        )
}