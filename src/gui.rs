use const_format::formatcp;
use web_view::Content;

pub struct Gui {}

const HTML_TEMPLATE: &str = include_str!("../index.html");
const AUDIBLE_FRAME_TEMPLATE: &str = r#"
<div class="frame" style="height: {}%"></div>
"#;
const INAUDIBLE_FRAME_TEMPLATE: &str = r#"
<div class="frame zero"></div>
"#;
const BLUE_FRAME_CSS_TEMPLATE_FMT: fn(&str) -> String = |x| {
	format!(
		r#"
.frame:nth-child({x}) {{
    background-color: blue !important;
    height: 200% !important;
}}
"#
	)
};

impl Gui {
	pub fn new() -> Gui {
		/* let bounds = split_streams(filename, frames.as_slice());
		let bounds = bounds
			.into_iter()
			.map(|x| BLUE_FRAME_CSS_TEMPLATE_FMT(x.frame + 1))
			.collect::<Vec<String>>()
			.join("\n");

		let divs = frames
		.iter()
		.map(|x| {
			if !x.is_audible() {
				(r#"<div class="frame zero"></div>"#).to_string()
			} else {
				format!(
					r#"<div class="frame" style="height: {}%"></div>"#,
					x.level() * 100.0
				)
			}
		})
		.collect::<Vec<_>>();

		let html = HTML_TEMPLATE.replace("{blue_frames}", &bounds);
		let html = html.replace("{frames}", &divs.join("\n"));
		let video = fs::read(filename).unwrap();
		let video = base64::engine::general_purpose::STANDARD_NO_PAD.encode(video);
		let html = html.replace("{video}", &video);

		*/

		let webbiew = web_view::builder()
			.title("My Project")
			.content(Content::Html("..."))
			.size(600, 600)
			.min_size(320, 180)
			.resizable(false)
			.debug(true)
			.user_data(())
			.invoke_handler(|_webview, _arg| Ok(()))
			.build()
			.unwrap();

		Gui {}
	}
}
