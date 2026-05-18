use askama::Template;
use termimad::MadSkin;

#[derive(Debug, Clone)]
pub struct UseView {
    pub alias: String,
    pub reference: String,
    pub digest: String,
    pub cache_path: String,
    pub transactions: Vec<String>,
    pub replaced: bool,
    pub dry_run: bool,
}

#[derive(Template)]
#[template(path = "use/added.md")]
struct UseTemplate<'a> {
    view: &'a UseView,
}

pub fn render(view: &UseView) {
    let markdown = UseTemplate { view }
        .render()
        .expect("Template rendering failed");
    let skin = MadSkin::default();
    skin.print_text(&markdown);
}
