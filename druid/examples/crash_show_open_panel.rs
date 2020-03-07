use druid::commands::SHOW_OPEN_PANEL;
use druid::widget::Button;
use druid::{
    AppLauncher, Command, EventCtx, FileDialogOptions, FileSpec, PlatformError, Widget, WindowDesc,
};

fn main() -> Result<(), PlatformError> {
    let window = WindowDesc::new(button);
    AppLauncher::with_window(window)
        .use_simple_logger()
        .launch(())
}

fn button() -> impl Widget<()> {
    Button::new("choose directory", |ctx: &mut EventCtx, _data, _env| {
        let opts = FileDialogOptions::new().allowed_types(vec![FileSpec::JPG]);
        ctx.submit_command(Command::new(SHOW_OPEN_PANEL, opts), None);
    })
}