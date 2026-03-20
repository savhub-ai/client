use ammonia::Builder;
use pulldown_cmark::{Options, Parser, html};

pub fn render_markdown(markdown: &str) -> String {
    let parser = Parser::new_ext(markdown, Options::all());
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    Builder::default()
        .add_tags([
            "section",
            "article",
            "aside",
            "figure",
            "figcaption",
            "time",
            "summary",
        ])
        .clean(&html_output)
        .to_string()
}
