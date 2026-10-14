/// A highlight category. It says what a piece of source is, not how it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxKind {
    Keyword,
    String,
    Number,
    Constant,
    Function,
    Type,
    Variable,
    Operator,
    Comment,
}

impl SyntaxKind {
    pub const ALL: [SyntaxKind; 9] = [
        SyntaxKind::Keyword,
        SyntaxKind::String,
        SyntaxKind::Number,
        SyntaxKind::Constant,
        SyntaxKind::Function,
        SyntaxKind::Type,
        SyntaxKind::Variable,
        SyntaxKind::Operator,
        SyntaxKind::Comment,
    ];

    /// The capture name queries use for this kind, without the leading `@`.
    pub const fn capture_name(self) -> &'static str {
        match self {
            SyntaxKind::Keyword => "keyword",
            SyntaxKind::String => "string",
            SyntaxKind::Number => "number",
            SyntaxKind::Constant => "constant",
            SyntaxKind::Function => "function",
            SyntaxKind::Type => "type",
            SyntaxKind::Variable => "variable",
            SyntaxKind::Operator => "operator",
            SyntaxKind::Comment => "comment",
        }
    }

    /// Resolves a query capture name, or `None` if it names no kind.
    pub fn from_capture_name(name: &str) -> Option<SyntaxKind> {
        SyntaxKind::ALL
            .into_iter()
            .find(|kind| kind.capture_name() == name)
    }
}
