//! `builtin:markdown-to-html` — Markdown (CommonMark + tables, footnotes, strikethrough,
//! task lists) to a standalone HTML page with a small readable stylesheet.
//!
//! Raw HTML in the Markdown is passed through, as Markdown allows; the result is a local
//! file the user opens themselves, with the same trust as the source.

use pulldown_cmark::{Options, Parser, html};

use super::io_err;
use crate::pipeline::StepContext;

const STYLE: &str = "body{font:16px/1.55 -apple-system,BlinkMacSystemFont,\"Helvetica Neue\",sans-serif;\
max-width:46em;margin:2em auto;padding:0 1em;color:#1d1d1f}\
pre,code{font-family:ui-monospace,Menlo,monospace;font-size:.92em}\
pre{background:#f5f5f7;padding:.8em 1em;border-radius:6px;overflow:auto}\
code{background:#f5f5f7;padding:.1em .3em;border-radius:4px}pre code{background:none;padding:0}\
table{border-collapse:collapse}th,td{border:1px solid #d2d2d7;padding:.3em .6em}\
blockquote{margin:0;padding-left:1em;border-left:3px solid #d2d2d7;color:#515154}\
img{max-width:100%}";

pub(super) fn run(ctx: &StepContext) -> Result<(), String> {
    let source =
        std::fs::read_to_string(&ctx.input).map_err(|e| io_err("cannot read the Markdown", e))?;
    std::fs::write(&ctx.output, render(&source, &ctx.stem))
        .map_err(|e| io_err("cannot write the HTML", e))
}

/// Renders `source` as a complete HTML document titled `title`.
pub fn render(source: &str, title: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let mut body = String::new();
    html::push_html(&mut body, Parser::new_ext(source, options));
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n\
         <style>{STYLE}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        escape(title)
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::test_util::ctx;

    #[test]
    fn renders_a_document() {
        let html = render(
            "# Hi\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n- [x] done",
            "My <notes>",
        );
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>My &lt;notes&gt;</title>"));
        assert!(html.contains("<h1>Hi</h1>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("checkbox"));
    }

    #[test]
    fn writes_the_output() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("readme.md");
        std::fs::write(&input, "*hello*").unwrap();
        let out = dir.path().join("readme.html");
        run(&ctx(&input, &out)).unwrap();
        let html = std::fs::read_to_string(out).unwrap();
        assert!(html.contains("<em>hello</em>") && html.contains("<title>readme</title>"));
    }
}
