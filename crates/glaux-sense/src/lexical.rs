//! Scans the text before the caret to find out whether it ends inside a comment or a
//! string, and how many blocks are still open.
//!
//! The parse tree cannot answer this reliably. An unclosed block comment parses as
//! code, and an unclosed Rhai string swallows the rest of the file. Scanning from the
//! start of the document does not have either problem.

use glaux_docs::ScriptLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Code,
    Comment,
    String,
    /// Inside a backtick string. The scan does not look for interpolations, so the
    /// parse tree decides whether the caret is in interpolated code.
    Template,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scan {
    pub state: State,
    /// Open blocks: `function ... end` and similar in Luau, `{ ... }` in Rhai.
    pub blocks: usize,
    /// Open parentheses and brackets, plus table braces in Luau.
    pub groups: usize,
}

/// Scans `text` from its start and reports where it ends.
pub(crate) fn scan(language: ScriptLanguage, text: &str) -> Scan {
    let mut scanner = Scanner {
        language,
        bytes: text.as_bytes(),
        at: 0,
        blocks: 0,
        groups: 0,
        previous: Token::Start,
    };
    let state = scanner.run();
    Scan {
        state,
        blocks: scanner.blocks,
        groups: scanner.groups,
    }
}

/// The previous token, which Luau needs to tell an `if` statement from an `if`
/// expression. Only the statement is closed by `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Start,
    /// A token that an expression follows, such as `=`, `(` or `return`.
    Operator,
    Other,
}

struct Scanner<'a> {
    language: ScriptLanguage,
    bytes: &'a [u8],
    at: usize,
    blocks: usize,
    groups: usize,
    previous: Token,
}

impl Scanner<'_> {
    fn run(&mut self) -> State {
        while self.at < self.bytes.len() {
            let byte = self.bytes[self.at];
            let state = match byte {
                b'"' | b'\'' => {
                    let single_line = byte == b'\'' || self.language == ScriptLanguage::Luau;
                    self.quoted(byte, single_line)
                }
                b'`' => self.template(),
                b'-' if self.language == ScriptLanguage::Luau && self.peek(1) == Some(b'-') => {
                    self.luau_comment()
                }
                b'/' if self.language == ScriptLanguage::Rhai && self.peek(1) == Some(b'/') => {
                    self.line_comment()
                }
                b'/' if self.language == ScriptLanguage::Rhai && self.peek(1) == Some(b'*') => {
                    self.rhai_block_comment()
                }
                b'[' if self.language == ScriptLanguage::Luau && self.long_bracket().is_some() => {
                    self.luau_long_string()
                }
                b'#' if self.language == ScriptLanguage::Rhai
                    && self.raw_string_hashes().is_some() =>
                {
                    self.rhai_raw_string()
                }
                _ => {
                    self.code(byte);
                    State::Code
                }
            };
            if state != State::Code {
                return state;
            }
        }
        State::Code
    }

    fn peek(&self, ahead: usize) -> Option<u8> {
        self.bytes.get(self.at + ahead).copied()
    }

    fn code(&mut self, byte: u8) {
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = self.at;
            while self.at < self.bytes.len() && is_word(self.bytes[self.at]) {
                self.at += 1;
            }
            let word = &self.bytes[start..self.at];
            let after_dot = start > 0 && matches!(self.bytes[start - 1], b'.' | b':');
            if self.language == ScriptLanguage::Luau && !after_dot {
                self.luau_word(word);
            } else {
                self.previous = Token::Other;
            }
            return;
        }
        self.at += 1;
        match byte {
            b'(' | b'[' => self.groups += 1,
            b')' | b']' => self.groups = self.groups.saturating_sub(1),
            b'{' if self.language == ScriptLanguage::Luau => self.groups += 1,
            b'}' if self.language == ScriptLanguage::Luau => {
                self.groups = self.groups.saturating_sub(1);
            }
            b'{' => self.blocks += 1,
            b'}' => self.blocks = self.blocks.saturating_sub(1),
            _ => {}
        }
        self.previous = match byte {
            b if b.is_ascii_whitespace() => self.previous,
            b'=' | b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'<' | b'>' | b'~' | b'&' | b'|'
            | b',' | b'(' | b'[' | b'{' | b'.' | b':' | b'?' => Token::Operator,
            _ => Token::Other,
        };
    }

    fn luau_word(&mut self, word: &[u8]) {
        match word {
            b"function" | b"do" | b"repeat" => self.blocks += 1,
            b"if" if self.previous != Token::Operator => self.blocks += 1,
            b"end" | b"until" => self.blocks = self.blocks.saturating_sub(1),
            _ => {}
        }
        self.previous = match word {
            b"return" | b"in" | b"and" | b"or" | b"not" | b"until" => Token::Operator,
            _ => Token::Other,
        };
    }

    /// Skips a quoted string, returning `String` if the text ends inside it.
    fn quoted(&mut self, quote: u8, ends_at_newline: bool) -> State {
        self.at += 1;
        while self.at < self.bytes.len() {
            match self.bytes[self.at] {
                b'\\' => self.at += 2,
                b'\n' if ends_at_newline => {
                    self.at += 1;
                    self.previous = Token::Other;
                    return State::Code;
                }
                byte if byte == quote => {
                    self.at += 1;
                    self.previous = Token::Other;
                    return State::Code;
                }
                _ => self.at += 1,
            }
        }
        State::String
    }

    fn template(&mut self) -> State {
        self.at += 1;
        while self.at < self.bytes.len() {
            match self.bytes[self.at] {
                b'\\' => self.at += 2,
                b'`' => {
                    self.at += 1;
                    self.previous = Token::Other;
                    return State::Code;
                }
                _ => self.at += 1,
            }
        }
        State::Template
    }

    fn line_comment(&mut self) -> State {
        while self.at < self.bytes.len() && self.bytes[self.at] != b'\n' {
            self.at += 1;
        }
        if self.at >= self.bytes.len() {
            return State::Comment;
        }
        State::Code
    }

    fn luau_comment(&mut self) -> State {
        self.at += 2;
        let Some(level) = self.long_bracket() else {
            return self.line_comment();
        };
        self.at += level + 2;
        self.close_long(level, State::Comment)
    }

    fn luau_long_string(&mut self) -> State {
        let level = self.long_bracket().unwrap_or(0);
        self.at += level + 2;
        let state = self.close_long(level, State::String);
        self.previous = Token::Other;
        state
    }

    /// The level of the long bracket at the scan position, where `[[` is 0 and `[==[` is 2.
    fn long_bracket(&self) -> Option<usize> {
        if self.peek(0) != Some(b'[') {
            return None;
        }
        let mut level = 0;
        while self.peek(1 + level) == Some(b'=') {
            level += 1;
        }
        (self.peek(1 + level) == Some(b'[')).then_some(level)
    }

    /// Skips to the closing bracket of `level`, or returns `inside` if the text ends first.
    fn close_long(&mut self, level: usize, inside: State) -> State {
        while self.at < self.bytes.len() {
            if self.bytes[self.at] == b']' {
                let equals = self.bytes[self.at + 1..]
                    .iter()
                    .take_while(|&&byte| byte == b'=')
                    .count();
                if equals == level && self.bytes.get(self.at + 1 + level) == Some(&b']') {
                    self.at += level + 2;
                    return State::Code;
                }
            }
            self.at += 1;
        }
        inside
    }

    /// Rhai block comments can nest.
    fn rhai_block_comment(&mut self) -> State {
        self.at += 2;
        let mut depth = 1;
        while self.at < self.bytes.len() {
            match (self.bytes[self.at], self.peek(1)) {
                (b'/', Some(b'*')) => {
                    depth += 1;
                    self.at += 2;
                }
                (b'*', Some(b'/')) => {
                    depth -= 1;
                    self.at += 2;
                    if depth == 0 {
                        return State::Code;
                    }
                }
                _ => self.at += 1,
            }
        }
        State::Comment
    }

    /// The number of `#` that open a Rhai raw string such as `#"..."#`.
    fn raw_string_hashes(&self) -> Option<usize> {
        let mut hashes = 0;
        while self.peek(hashes) == Some(b'#') {
            hashes += 1;
        }
        (self.peek(hashes) == Some(b'"')).then_some(hashes)
    }

    fn rhai_raw_string(&mut self) -> State {
        let hashes = self.raw_string_hashes().unwrap_or(0);
        self.at += hashes + 1;
        while self.at < self.bytes.len() {
            if self.bytes[self.at] == b'"'
                && self.bytes[self.at + 1..]
                    .iter()
                    .take_while(|&&byte| byte == b'#')
                    .count()
                    >= hashes
            {
                self.at += 1 + hashes;
                self.previous = Token::Other;
                return State::Code;
            }
            self.at += 1;
        }
        State::String
    }
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests;
