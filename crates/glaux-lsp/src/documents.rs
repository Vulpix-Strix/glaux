//! The documents a session has open and the notifications still owed to the
//! server.
//!
//! [`Documents`] is the single copy of what the client believes the server
//! should know. The main thread edits it under a lock and the session task
//! drains it, so neither ever waits for the other's I/O.
//!
//! Draining preserves each document's lifecycle: a `didOpen` is never
//! dropped, a `didClose` is never dropped, nothing is sent for a document
//! before its open or after its close. Only consecutive changes merge. They
//! are always full text, so the latest one supersedes the rest.

use std::collections::{BTreeMap, VecDeque};

/// Too many lifecycle notifications are waiting for the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the notification queue is full")]
pub struct QueueFull;

/// A notification ready to be written to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Wire {
    Open {
        uri: String,
        language: String,
        version: i64,
        text: String,
    },
    Change {
        uri: String,
        version: i64,
        text: String,
    },
    Close {
        uri: String,
    },
}

#[derive(Debug)]
struct Document {
    version: i64,
    text: String,
}

#[derive(Debug)]
enum Owed {
    /// Carries its own text so a close queued behind it cannot orphan it.
    Open {
        uri: String,
        language: String,
        version: i64,
        text: String,
    },
    /// The text is read from the document when drained.
    Change {
        uri: String,
    },
    Close {
        uri: String,
    },
}

impl Owed {
    fn uri(&self) -> &str {
        match self {
            Self::Open { uri, .. } | Self::Change { uri } | Self::Close { uri } => uri,
        }
    }
}

#[derive(Debug)]
pub struct Documents {
    open: BTreeMap<String, Document>,
    language: BTreeMap<String, String>,
    owed: VecDeque<Owed>,
    capacity: usize,
}

impl Documents {
    pub fn new(capacity: usize) -> Self {
        Self {
            open: BTreeMap::new(),
            language: BTreeMap::new(),
            owed: VecDeque::new(),
            capacity,
        }
    }

    /// Opens a document, or replaces the text of one that is already open.
    pub fn open(
        &mut self,
        uri: &str,
        language: &str,
        version: i64,
        text: &str,
    ) -> Result<(), QueueFull> {
        if self.open.contains_key(uri) {
            self.change(uri, version, text);
            return Ok(());
        }
        if self.owed.len() >= self.capacity {
            return Err(QueueFull);
        }
        self.open.insert(
            uri.to_owned(),
            Document {
                version,
                text: text.to_owned(),
            },
        );
        self.language.insert(uri.to_owned(), language.to_owned());
        self.owed.push_back(Owed::Open {
            uri: uri.to_owned(),
            language: language.to_owned(),
            version,
            text: text.to_owned(),
        });
        Ok(())
    }

    /// Replaces the text of an open document. Changes to a document that is
    /// not open are ignored, since the server has never heard of it.
    pub fn change(&mut self, uri: &str, version: i64, text: &str) {
        let Some(document) = self.open.get_mut(uri) else {
            return;
        };
        document.version = version;
        document.text = text.to_owned();
        match self.owed.iter_mut().rev().find(|owed| owed.uri() == uri) {
            Some(Owed::Open {
                version: queued_version,
                text: queued_text,
                ..
            }) => {
                *queued_version = version;
                text.clone_into(queued_text);
            }
            Some(Owed::Change { .. }) => {}
            Some(Owed::Close { .. }) | None => self.owed.push_back(Owed::Change {
                uri: uri.to_owned(),
            }),
        }
    }

    /// Closes a document. Closing one that is not open is ignored.
    pub fn close(&mut self, uri: &str) -> Result<(), QueueFull> {
        if !self.open.contains_key(uri) {
            return Ok(());
        }
        if self.owed.len() >= self.capacity {
            return Err(QueueFull);
        }
        self.open.remove(uri);
        self.language.remove(uri);
        self.owed
            .retain(|owed| !matches!(owed, Owed::Change { uri: changed } if changed == uri));
        self.owed.push_back(Owed::Close {
            uri: uri.to_owned(),
        });
        Ok(())
    }

    /// Takes every notification owed to the server, in order.
    pub(crate) fn drain(&mut self) -> Vec<Wire> {
        let mut wire = Vec::with_capacity(self.owed.len());
        for owed in self.owed.drain(..) {
            match owed {
                Owed::Open {
                    uri,
                    language,
                    version,
                    text,
                } => wire.push(Wire::Open {
                    uri,
                    language,
                    version,
                    text,
                }),
                Owed::Change { uri } => {
                    if let Some(document) = self.open.get(&uri) {
                        wire.push(Wire::Change {
                            version: document.version,
                            text: document.text.clone(),
                            uri,
                        });
                    }
                }
                Owed::Close { uri } => wire.push(Wire::Close { uri }),
            }
        }
        wire
    }

    /// Forgets what the server was told and owes it every open document
    /// again. Used when a new server process replaces a dead one.
    pub(crate) fn resync(&mut self) {
        self.owed.clear();
        for (uri, document) in &self.open {
            self.owed.push_back(Owed::Open {
                uri: uri.clone(),
                language: self.language.get(uri).cloned().unwrap_or_default(),
                version: document.version,
                text: document.text.clone(),
            });
        }
    }

    pub fn has_owed(&self) -> bool {
        !self.owed.is_empty()
    }

    /// The open documents and their latest versions.
    pub fn versions(&self) -> Vec<(String, i64)> {
        self.open
            .iter()
            .map(|(uri, document)| (uri.clone(), document.version))
            .collect()
    }

    pub fn version(&self, uri: &str) -> Option<i64> {
        self.open.get(uri).map(|document| document.version)
    }
}

#[cfg(test)]
mod tests;
