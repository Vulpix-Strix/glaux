//! Reads SPDX license expressions far enough to tell which licenses a
//! component has to follow and which it only offers as options.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Expression {
    /// One license with its `+` suffix and any `WITH` exception, as written,
    /// so `Apache-2.0 WITH LLVM-exception` counts as a single license.
    License(String),
    And(Vec<Expression>),
    Or(Vec<Expression>),
}

/// How a component's license expression binds the people who use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Terms {
    Single,
    All(usize),
    Choice(usize),
}

/// The licenses an expression mentions, split by the role they play.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Roles {
    /// Licenses that always apply, which are a lone license or any part of
    /// an `AND` that is not under an `OR`.
    pub(super) required: Vec<String>,
    /// Licenses offered as one choice among others, at any depth under an
    /// `OR`.
    pub(super) offered: Vec<String>,
}

impl Expression {
    /// Parses `text`. Text that is not a well-formed expression is kept
    /// whole as one license, so nothing is lost or invented.
    pub(super) fn parse(text: &str) -> Self {
        let text = text.trim();
        let mut parser = Parser {
            tokens: tokens(text),
            at: 0,
        };
        match parser.or() {
            Some(expression) if parser.at == parser.tokens.len() => expression,
            _ => Self::License(text.to_owned()),
        }
    }

    pub(super) fn terms(&self) -> Terms {
        match self {
            Self::License(_) => Terms::Single,
            Self::And(parts) => Terms::All(parts.len()),
            Self::Or(parts) => Terms::Choice(parts.len()),
        }
    }

    pub(super) fn roles(&self) -> Roles {
        let mut roles = Roles::default();
        self.collect(false, &mut roles);
        roles
    }

    fn collect(&self, offered: bool, roles: &mut Roles) {
        match self {
            Self::License(id) => {
                let list = if offered {
                    &mut roles.offered
                } else {
                    &mut roles.required
                };
                if !list.contains(id) {
                    list.push(id.clone());
                }
            }
            Self::And(parts) => parts.iter().for_each(|part| part.collect(offered, roles)),
            Self::Or(parts) => parts.iter().for_each(|part| part.collect(true, roles)),
        }
    }
}

fn tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, tokens: &mut Vec<String>| {
        if !word.is_empty() {
            tokens.push(std::mem::take(word));
        }
    };
    for ch in text.chars() {
        match ch {
            '(' | ')' => {
                flush(&mut word, &mut tokens);
                tokens.push(ch.to_string());
            }
            ch if ch.is_whitespace() => flush(&mut word, &mut tokens),
            ch => word.push(ch),
        }
    }
    flush(&mut word, &mut tokens);
    tokens
}

struct Parser {
    tokens: Vec<String>,
    at: usize,
}

impl Parser {
    fn peek_is(&self, word: &str) -> bool {
        self.tokens
            .get(self.at)
            .is_some_and(|token| token.eq_ignore_ascii_case(word))
    }

    fn or(&mut self) -> Option<Expression> {
        let mut parts = vec![self.and()?];
        while self.peek_is("OR") {
            self.at += 1;
            parts.push(self.and()?);
        }
        Some(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Expression::Or(parts)
        })
    }

    fn and(&mut self) -> Option<Expression> {
        let mut parts = vec![self.term()?];
        while self.peek_is("AND") {
            self.at += 1;
            parts.push(self.term()?);
        }
        Some(if parts.len() == 1 {
            parts.remove(0)
        } else {
            Expression::And(parts)
        })
    }

    fn term(&mut self) -> Option<Expression> {
        let token = self.tokens.get(self.at)?.clone();
        self.at += 1;
        match token.as_str() {
            "(" => {
                let inner = self.or()?;
                if self.tokens.get(self.at).map(String::as_str) != Some(")") {
                    return None;
                }
                self.at += 1;
                Some(inner)
            }
            ")" => None,
            word if ["AND", "OR", "WITH"]
                .iter()
                .any(|op| word.eq_ignore_ascii_case(op)) =>
            {
                None
            }
            _ => {
                let mut id = token;
                if self.peek_is("WITH") {
                    let exception = self.tokens.get(self.at + 1)?;
                    id = format!("{id} WITH {exception}");
                    self.at += 2;
                }
                Some(Expression::License(id))
            }
        }
    }
}

#[cfg(test)]
mod tests;
