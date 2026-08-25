use omv_syntax::HighlightKind;
use ratatui::style::{Color, Modifier, Style};

/// Colours for syntax and chrome. One struct so a `themes/` directory can later
/// deserialize straight into it without touching any render code.
#[derive(Debug, Clone)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub gutter: Color,
    pub gutter_current: Color,
    pub cursor_line: Color,
    pub selection: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub mode_normal: Color,
    pub mode_insert: Color,
    pub mode_visual: Color,
    pub mode_command: Color,
    pub panel_border: Color,
    pub panel_title: Color,
    pub match_highlight: Color,
    pub directory: Color,
    pub error: Color,
    pub warning: Color,
    pub info: Color,
    pub hint: Color,

    pub keyword: Color,
    pub function: Color,
    pub type_: Color,
    pub constructor: Color,
    pub variable: Color,
    pub property: Color,
    pub parameter: Color,
    pub string: Color,
    pub number: Color,
    pub boolean: Color,
    pub comment: Color,
    pub operator: Color,
    pub punctuation: Color,
    pub attribute: Color,
    pub constant: Color,
}

impl Theme {
    /// A dark default in the 256-colour space, so it looks the same in any
    /// terminal that isn't truecolor-capable.
    pub fn default_dark() -> Self {
        Theme {
            background: Color::Reset,
            foreground: Color::Rgb(0xc8, 0xd3, 0xf5),
            gutter: Color::Rgb(0x3b, 0x44, 0x61),
            gutter_current: Color::Rgb(0xff, 0xc7, 0x77),
            cursor_line: Color::Rgb(0x22, 0x26, 0x36),
            selection: Color::Rgb(0x2d, 0x3f, 0x76),
            status_bg: Color::Rgb(0x1e, 0x20, 0x30),
            status_fg: Color::Rgb(0xc8, 0xd3, 0xf5),
            mode_normal: Color::Rgb(0x82, 0xaa, 0xff),
            mode_insert: Color::Rgb(0xc3, 0xe8, 0x8d),
            mode_visual: Color::Rgb(0xc0, 0x99, 0xff),
            mode_command: Color::Rgb(0xff, 0xc7, 0x77),
            panel_border: Color::Rgb(0x58, 0x9e, 0xd7),
            panel_title: Color::Rgb(0x82, 0xaa, 0xff),
            match_highlight: Color::Rgb(0xff, 0xc7, 0x77),
            directory: Color::Rgb(0x82, 0xaa, 0xff),
            error: Color::Rgb(0xff, 0x75, 0x7f),
            warning: Color::Rgb(0xff, 0xc7, 0x77),
            info: Color::Rgb(0x0d, 0xb9, 0xd7),
            hint: Color::Rgb(0x4f, 0xd6, 0xbe),

            keyword: Color::Rgb(0xfc, 0xa7, 0xea),
            function: Color::Rgb(0x82, 0xaa, 0xff),
            type_: Color::Rgb(0xff, 0xc7, 0x77),
            constructor: Color::Rgb(0xff, 0xc7, 0x77),
            variable: Color::Rgb(0xc8, 0xd3, 0xf5),
            property: Color::Rgb(0x4f, 0xd6, 0xbe),
            parameter: Color::Rgb(0xe0, 0xaf, 0x68),
            string: Color::Rgb(0xc3, 0xe8, 0x8d),
            number: Color::Rgb(0xff, 0x98, 0x5a),
            boolean: Color::Rgb(0xff, 0x98, 0x5a),
            comment: Color::Rgb(0x63, 0x6d, 0xa6),
            operator: Color::Rgb(0x89, 0xdd, 0xff),
            punctuation: Color::Rgb(0x89, 0xdd, 0xff),
            attribute: Color::Rgb(0xc0, 0x99, 0xff),
            constant: Color::Rgb(0xff, 0x98, 0x5a),
        }
    }

    pub fn style_for(&self, kind: HighlightKind) -> Style {
        use HighlightKind::*;
        let color = match kind {
            Keyword => self.keyword,
            Function => self.function,
            Type => self.type_,
            Constructor => self.constructor,
            Variable => self.variable,
            Property => self.property,
            Parameter => self.parameter,
            String => self.string,
            Number => self.number,
            Boolean => self.boolean,
            Comment => self.comment,
            Operator => self.operator,
            Punctuation => self.punctuation,
            Attribute => self.attribute,
            Constant => self.constant,
        };
        let style = Style::default().fg(color);
        if kind == Comment {
            style.add_modifier(Modifier::ITALIC)
        } else {
            style
        }
    }

    pub fn mode_color(&self, mode: omv_core::Mode) -> Color {
        use omv_core::Mode::*;
        match mode {
            Normal => self.mode_normal,
            Insert => self.mode_insert,
            Visual | VisualLine => self.mode_visual,
            Command => self.mode_command,
        }
    }

    pub fn severity_color(
        &self,
        severity: Option<omv_lsp::lsp_types::DiagnosticSeverity>,
    ) -> Color {
        use omv_lsp::lsp_types::DiagnosticSeverity as S;
        match severity {
            Some(S::ERROR) | None => self.error,
            Some(S::WARNING) => self.warning,
            Some(S::INFORMATION) => self.info,
            _ => self.hint,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::default_dark()
    }
}
