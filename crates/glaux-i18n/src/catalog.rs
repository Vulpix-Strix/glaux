use std::collections::{BTreeMap, BTreeSet};

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use fluent_syntax::ast;
use fluent_syntax::parser;

use crate::{CatalogError, Language, SyntaxIssue};

/// The text substituted for every variable in one of the sample passes.
pub const SAMPLE_TEXT: &str = "x";

/// Numbers substituted for every variable in the other sample passes. They
/// cover the plural categories of English and Spanish: zero, one, two, a
/// number ending in one, and a large round number.
pub const SAMPLE_NUMBERS: [f64; 5] = [0.0, 1.0, 2.0, 21.0, 1_000_000.0];

/// What a message needs from its caller and the literal text it contains.
#[derive(Debug, Clone, Default)]
struct MessageInfo {
    /// Variables referenced by the value or any attribute.
    variables: BTreeSet<String>,
    /// Attribute names.
    attributes: Vec<String>,
    /// Every literal text run, including variants a sample pass cannot reach.
    literals: Vec<String>,
}

/// The messages of one language, parsed from Fluent source.
pub struct Catalog {
    language: Language,
    bundle: FluentBundle<FluentResource>,
    messages: BTreeMap<String, MessageInfo>,
}

/// The texts one message can produce, and what went wrong producing them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Samples {
    /// Resolved values and attributes for each sample pass, followed by the
    /// literal text of every variant. Order is stable, duplicates are kept
    /// out.
    pub texts: Vec<String>,
    /// Formatting problems such as an unknown reference.
    pub errors: Vec<String>,
}

impl Catalog {
    /// Parses a catalog. Syntax errors and duplicate messages are reported
    /// together rather than one at a time.
    pub fn parse(language: Language, source: &str) -> Result<Self, CatalogError> {
        let syntax = match parser::parse(source) {
            Ok(syntax) => syntax,
            Err((_, errors)) => {
                let issues = errors
                    .iter()
                    .map(|error| SyntaxIssue {
                        line: line_of(source, error.pos.start),
                        message: error.to_string(),
                    })
                    .collect();
                return Err(CatalogError::Syntax { language, issues });
            }
        };

        let mut messages = BTreeMap::new();
        for entry in &syntax.body {
            if let ast::Entry::Message(message) = entry {
                let id = message.id.name.to_owned();
                if messages.insert(id.clone(), message_info(message)).is_some() {
                    return Err(CatalogError::DuplicateMessage { language, id });
                }
            }
        }

        let resource = FluentResource::try_new(source.to_owned())
            .map_err(|(_, errors)| syntax_error(&language, source, errors.len()))?;
        let mut bundle = FluentBundle::new(vec![language.identifier().clone()]);
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .map_err(|_| syntax_error(&language, source, 1))?;

        Ok(Self {
            language,
            bundle,
            messages,
        })
    }

    pub fn language(&self) -> &Language {
        &self.language
    }

    /// Message ids in sorted order. Terms are private to a catalog and are
    /// not listed.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.messages.keys().map(String::as_str)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.messages.contains_key(id)
    }

    /// The variables a message expects, or `None` for an unknown id.
    pub fn variables(&self, id: &str) -> Option<&BTreeSet<String>> {
        self.messages.get(id).map(|info| &info.variables)
    }

    /// Formats the value of `id`, or `None` when the message is unknown or
    /// has no value. Formatting problems still yield text, with the
    /// offending placeable left in place.
    pub fn format(&self, id: &str, args: &FluentArgs<'_>) -> Option<String> {
        let pattern = self.bundle.get_message(id)?.value()?;
        let mut errors = Vec::new();
        Some(
            self.bundle
                .format_pattern(pattern, Some(args), &mut errors)
                .into_owned(),
        )
    }

    /// Every text `id` can produce, for checking against a font.
    ///
    /// The value and each attribute are formatted once with every variable
    /// set to [`SAMPLE_TEXT`] and once per entry of [`SAMPLE_NUMBERS`]. Text
    /// in variants those passes do not select is added from the source, so
    /// no branch escapes the check. Returns `None` for an unknown id.
    pub fn resolve_samples(&self, id: &str) -> Option<Samples> {
        let info = self.messages.get(id)?;
        let message = self.bundle.get_message(id)?;
        let mut samples = Samples::default();

        let mut patterns = Vec::new();
        if let Some(value) = message.value() {
            patterns.push(value);
        }
        for name in &info.attributes {
            if let Some(attribute) = message.get_attribute(name) {
                patterns.push(attribute.value());
            }
        }

        let passes = std::iter::once(FluentValue::from(SAMPLE_TEXT))
            .chain(SAMPLE_NUMBERS.iter().map(|n| FluentValue::from(*n)));
        for value in passes {
            let mut args = FluentArgs::new();
            for variable in &info.variables {
                args.set(variable.as_str(), value.clone());
            }
            for pattern in &patterns {
                let mut errors = Vec::new();
                let text = self
                    .bundle
                    .format_pattern(pattern, Some(&args), &mut errors)
                    .into_owned();
                push_unique(&mut samples.texts, text);
                for error in errors {
                    push_unique(&mut samples.errors, error.to_string());
                }
            }
        }
        for literal in &info.literals {
            push_unique(&mut samples.texts, literal.clone());
        }
        Some(samples)
    }
}

fn push_unique(list: &mut Vec<String>, item: String) {
    if !list.contains(&item) {
        list.push(item);
    }
}

fn line_of(source: &str, byte: usize) -> usize {
    let end = byte.min(source.len());
    source.as_bytes()[..end]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

fn syntax_error(language: &Language, source: &str, count: usize) -> CatalogError {
    CatalogError::Syntax {
        language: language.clone(),
        issues: vec![SyntaxIssue {
            line: line_of(source, 0),
            message: format!("catalog could not be loaded ({count} problem(s))"),
        }],
    }
}

fn message_info(message: &ast::Message<&str>) -> MessageInfo {
    let mut info = MessageInfo::default();
    if let Some(value) = &message.value {
        collect_pattern(value, &mut info);
    }
    for attribute in &message.attributes {
        info.attributes.push(attribute.id.name.to_owned());
        collect_pattern(&attribute.value, &mut info);
    }
    info
}

fn collect_pattern(pattern: &ast::Pattern<&str>, info: &mut MessageInfo) {
    for element in &pattern.elements {
        match element {
            ast::PatternElement::TextElement { value } => info.literals.push((*value).to_owned()),
            ast::PatternElement::Placeable { expression } => collect_expression(expression, info),
        }
    }
}

fn collect_expression(expression: &ast::Expression<&str>, info: &mut MessageInfo) {
    match expression {
        ast::Expression::Inline(inline) => collect_inline(inline, info),
        ast::Expression::Select { selector, variants } => {
            collect_inline(selector, info);
            for variant in variants {
                collect_pattern(&variant.value, info);
            }
        }
    }
}

fn collect_inline(inline: &ast::InlineExpression<&str>, info: &mut MessageInfo) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            info.variables.insert(id.name.to_owned());
        }
        ast::InlineExpression::FunctionReference { arguments, .. } => {
            collect_arguments(arguments, info);
        }
        ast::InlineExpression::TermReference {
            arguments: Some(arguments),
            ..
        } => collect_arguments(arguments, info),
        ast::InlineExpression::Placeable { expression } => collect_expression(expression, info),
        ast::InlineExpression::StringLiteral { .. }
        | ast::InlineExpression::NumberLiteral { .. }
        | ast::InlineExpression::MessageReference { .. }
        | ast::InlineExpression::TermReference { .. } => {}
    }
}

fn collect_arguments(arguments: &ast::CallArguments<&str>, info: &mut MessageInfo) {
    for positional in &arguments.positional {
        collect_inline(positional, info);
    }
    for named in &arguments.named {
        collect_inline(&named.value, info);
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn catalog(source: &str) -> Catalog {
        Catalog::parse(Language::english(), source).unwrap()
    }

    #[test]
    fn samples_reach_every_plural_branch() {
        let catalog = catalog(
            "n = { $count ->\n    [one] { $count } file\n   *[other] { $count } files\n}\n",
        );
        let samples = catalog.resolve_samples("n").unwrap();
        assert!(samples.texts.contains(&"1 file".to_owned()), "{samples:?}");
        assert!(samples.texts.contains(&"2 files".to_owned()), "{samples:?}");
        assert_eq!(samples.errors, Vec::<String>::new());
    }

    #[test]
    fn samples_include_text_of_variants_no_sample_selects() {
        let catalog = catalog("kind = { $kind ->\n    [file] a file\n   *[other] something\n}\n");
        let samples = catalog.resolve_samples("kind").unwrap();
        assert!(
            samples.texts.iter().any(|text| text.contains("a file")),
            "{samples:?}"
        );
    }

    #[test]
    fn samples_include_attributes() {
        let catalog = catalog("save = Save\n    .title = Write { $name } to disk\n");
        let samples = catalog.resolve_samples("save").unwrap();
        assert!(
            samples.texts.contains(&"Write x to disk".to_owned()),
            "{samples:?}"
        );
    }

    #[test]
    fn unknown_id_has_no_samples() {
        assert_eq!(catalog("a = A\n").resolve_samples("b"), None);
    }

    #[test]
    fn variables_are_collected_through_selectors_and_attributes() {
        let catalog =
            catalog("m = { $a ->\n    [x] { $b }\n   *[other] none\n}\n    .attr = { $c }\n");
        let variables: Vec<_> = catalog.variables("m").unwrap().iter().cloned().collect();
        assert_eq!(variables, ["a", "b", "c"]);
    }
}
