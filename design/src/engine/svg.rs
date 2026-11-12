use super::*;

/// The mark, for a gateway that serves a page with no icon of its own.
pub const FAVICON_SVG: &str = include_str!("../../assets/favicon.svg");

/// The mark: the Face at rest. Its eyes take the canvas colour of the viewer's scheme.
pub fn mark_svg() -> String {
    let c = &tokens().color;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\" role=\"img\" aria-label=\"hbots\">{}{}</svg>\n",
        scheme_style("eye", &c.paper, &c.night),
        face_shapes(0.0, 0.0, 1.0, "class=\"eye\"")
    )
}

/// The mark for a browser tab: eyes in Night, which reads on a light or a dark tab strip.
pub fn favicon_svg() -> String {
    let c = &tokens().color;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\">{}</svg>\n",
        face_shapes(0.0, 0.0, 1.0, &format!("fill=\"{}\"", c.night))
    )
}

/// The app icon: the mark on Night, square, for the platform to mask.
pub fn app_icon_svg() -> String {
    let c = &tokens().color;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1024 1024\" role=\"img\" aria-label=\"hbots\"><rect width=\"1024\" height=\"1024\" fill=\"{}\"/>{}</svg>\n",
        c.night,
        face_shapes(212.0, 212.0, 6.0, &format!("fill=\"{}\"", c.night))
    )
}

/// The lockup: the mark and the name beside it.
pub fn lockup_svg() -> String {
    let t = tokens();
    let c = &t.color;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 330 100\" role=\"img\" aria-label=\"{name}\">{eye}{word}{face}<text class=\"word\" x=\"122\" y=\"71\" font-family=\"system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif\" font-size=\"64\" font-weight=\"700\" letter-spacing=\"-2\">{name}</text></svg>\n",
        name = t.brand.name,
        eye = scheme_style("eye", &c.paper, &c.night),
        word = scheme_style("word", &c.night, &c.paper),
        face = face_shapes(0.0, 0.0, 1.0, "class=\"eye\""),
    )
}
